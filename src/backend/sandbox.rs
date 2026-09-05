use std::path::Path;

// bwrap costs a few ms over a bare exec here and systemd-run costs 23.8 ms; see AGENTS.md "Thumbnail sandbox".
const BWRAP: &str = "bwrap";
// bwrap has no rlimit option, so stock prlimit carries them; see AGENTS.md "Thumbnail sandbox".
const PRLIMIT: &str = "prlimit";
// A 1080p decode is well under a second of CPU here, so 30 s is a runaway, not a slow file.
const CPU_SECONDS: u32 = 30;
// Issue #17 reports glycin exhausting 1 GiB of address space on a large ICC-tagged JPEG and aborting, which this box does not reproduce, so the cap is 2 GiB: the smallest value the ticket records as working, still finite, and virtual rather than resident.
const ADDRESS_SPACE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

// /bin, /sbin, /lib and /lib64 are all symlinks into usr on this box, so binding /usr covers them.
const BWRAP_FLAGS: &[&str] = &[
    "--unshare-all",
    "--die-with-parent",
    "--new-session",
    "--clearenv",
    "--ro-bind",
    "/usr",
    "/usr",
    "--ro-bind",
    "/etc",
    "/etc",
    "--symlink",
    "usr/lib",
    "/lib",
    "--symlink",
    "usr/lib",
    "/lib64",
    "--symlink",
    "usr/bin",
    "/bin",
    "--symlink",
    "usr/bin",
    "/sbin",
    "--proc",
    "/proc",
    "--dev",
    "/dev",
    "--tmpfs",
    "/tmp",
];

pub fn available() -> bool {
    available_on(&std::env::var("PATH").unwrap_or_default())
}

// Split from available() so a test can ask the rule without setting PATH for every thread beside it.
// Sample input: "/usr/local/bin:/usr/bin:/bin"
fn available_on(path: &str) -> bool {
    let has = |prog: &str| {
        path.split(':')
            .filter(|d| !d.is_empty())
            .any(|d| Path::new(d).join(prog).is_file())
    };
    has(BWRAP) && has(PRLIMIT)
}

// The input is read-only, the one path the caller names is the only writable one, and nothing else is shared.
pub fn wrap(inner: &[String], input: &Path, out: &Path) -> Vec<String> {
    let head_and_binds = 10;
    let mut a: Vec<String> = Vec::with_capacity(inner.len() + BWRAP_FLAGS.len() + head_and_binds);
    a.push(PRLIMIT.to_string());
    a.push(format!("--cpu={}", CPU_SECONDS));
    a.push(format!("--as={}", ADDRESS_SPACE_BYTES));
    a.push(BWRAP.to_string());
    for flag in BWRAP_FLAGS {
        a.push(flag.to_string());
    }
    a.push("--ro-bind".to_string());
    a.push(input.to_string_lossy().to_string());
    a.push(input.to_string_lossy().to_string());
    // The caller names the one writable path, and production binds the single pre-created temp file; see AGENTS.md "Thumbnail sandbox".
    a.push("--bind".to_string());
    a.push(out.to_string_lossy().to_string());
    a.push(out.to_string_lossy().to_string());
    a.extend_from_slice(inner);
    a
}

// The same boundary with nothing writable at all, for a probe that answers on stdout rather than
// into a file. ffprobe parses the same untrusted media a thumbnailer does and gets the same jail.
pub fn wrap_readonly(inner: &[String], input: &Path) -> Vec<String> {
    let head_and_binds = 8;
    let mut a: Vec<String> = Vec::with_capacity(inner.len() + BWRAP_FLAGS.len() + head_and_binds);
    a.push(PRLIMIT.to_string());
    a.push(format!("--cpu={}", CPU_SECONDS));
    a.push(format!("--as={}", ADDRESS_SPACE_BYTES));
    a.push(BWRAP.to_string());
    for flag in BWRAP_FLAGS {
        a.push(flag.to_string());
    }
    a.push("--ro-bind".to_string());
    a.push(input.to_string_lossy().to_string());
    a.push(input.to_string_lossy().to_string());
    a.extend_from_slice(inner);
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // Written out rather than derived from the constant: a test that recomputes the value it checks cannot fail when that value is wrong.
    const TWO_GIB: &str = "--as=2147483648";
    // /proc reports VmPeak and VmRSS in kibibytes, and the reservations below are sized in mebibytes.
    const KIB_PER_GIB: u64 = 1024 * 1024;
    const BYTES_PER_KIB: u64 = 1024;
    // 256 MiB, twenty times the prober's measured resident size: a reservation must not become memory.
    const RESIDENT_CEILING_KIB: u64 = 262_144;
    // The one interpreter on this box that can ask the kernel for a mapping of a chosen protection.
    const PYTHON: &str = "/usr/bin/python3";

    // PROT_NONE with MAP_NORESERVE is address space and not one resident page, which is what a cap on address space bounds and what issue #17 says 1 GiB of was not enough of.
    const RESERVE_PROBE: &str = r#"
import ctypes
PROT_NONE, MAP_PRIVATE, MAP_ANONYMOUS, MAP_NORESERVE = 0, 0x02, 0x20, 0x4000
MIB, UNDER_MIB, OVER_MIB = 1024 * 1024, 1536, 3072
libc = ctypes.CDLL("libc.so.6")
libc.mmap.restype = ctypes.c_void_p
libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_long]
MAP_FAILED = ctypes.c_void_p(-1).value
def reserve(mib):
    got = libc.mmap(None, mib * MIB, PROT_NONE, MAP_PRIVATE | MAP_ANONYMOUS | MAP_NORESERVE, -1, 0)
    return "ok" if got != MAP_FAILED else "refused"
# Sample /proc/self/limits row: "Max address space         2147483648           2147483648           bytes"
def field(path, prefix, column):
    return next(l.split()[column] for l in open(path) if l.startswith(prefix))
print("cap=" + field("/proc/self/limits", "Max address space", 3))
print("under=" + reserve(UNDER_MIB))
print("VmPeakKb=" + field("/proc/self/status", "VmPeak:", 1))
print("VmRSSKb=" + field("/proc/self/status", "VmRSS:", 1))
print("over=" + reserve(OVER_MIB))
"#;

    // The jail is mandatory, so the one input that decides it is asserted rather than assumed: an empty PATH must read unavailable, or every caller's fail-closed branch is unreachable.
    #[test]
    fn a_path_without_the_two_tools_reads_unavailable() {
        assert!(!available_on(""), "an empty PATH cannot hold either tool");
        assert!(!available_on("::"), "empty components are skipped rather than treated as the root");
        assert!(!available_on("/nonexistent-dir-for-this-test"), "a directory holding neither is not enough");
    }

    fn inner() -> Vec<String> {
        vec![
            "/usr/bin/ffmpegthumbnailer".to_string(),
            "-i".to_string(),
            "/in/a.mp4".to_string(),
            "-o".to_string(),
            "/out/x.png".to_string(),
        ]
    }

    // The brief's "no argument contains sh" is false against a correct argv, because --unshare-all does.
    fn is_a_shell(a: &str) -> bool {
        a == "sh" || a == "bash" || a == "-c" || a.ends_with("/sh") || a.ends_with("/bash")
    }

    #[test]
    fn the_inner_argv_is_last_and_unchanged() {
        let got = wrap(&inner(), Path::new("/in/a.mp4"), Path::new("/out"));
        let tail = &got[got.len() - inner().len()..];
        assert_eq!(tail, inner().as_slice());
    }

    #[test]
    fn the_input_is_bound_read_only_and_the_output_directory_is_the_only_writable_path() {
        let got = wrap(&inner(), Path::new("/in/a.mp4"), Path::new("/out"));
        let joined = got.join(" ");
        assert!(joined.contains("--ro-bind /in/a.mp4 /in/a.mp4"));
        assert!(joined.contains("--bind /out /out"));
        // The input must never appear behind a writable bind.
        assert!(!joined.contains("--bind /in/a.mp4"));
    }

    #[test]
    fn the_namespace_and_lifetime_flags_are_present() {
        let got = wrap(&inner(), Path::new("/in/a.mp4"), Path::new("/out"));
        assert!(got.iter().any(|a| a == "--unshare-all"));
        assert!(got.iter().any(|a| a == "--die-with-parent"));
        assert!(got.iter().any(|a| a == "--new-session"));
    }

    #[test]
    fn the_rlimits_are_applied_outside_bwrap() {
        let got = wrap(&inner(), Path::new("/in/a.mp4"), Path::new("/out"));
        let readonly = wrap_readonly(&inner(), Path::new("/in/a.mp4"));
        assert_eq!(got[0], "prlimit");
        assert_eq!(readonly[0], "prlimit");
        assert!(got.iter().any(|a| a.starts_with("--cpu=")));
        assert!(readonly.iter().any(|a| a.starts_with("--cpu=")));
        // The exact value in both wrappers: "--as= has some value" passed at 1 GiB, so it could not see the cap itself being wrong.
        for a in [&got, &readonly] {
            assert!(a.iter().any(|x| x == TWO_GIB), "the wrapper caps address space at 2 GiB: {:?}", a);
        }
        // This pins prlimit before bwrap in the argv; see AGENTS.md "Thumbnail sandbox" for why.
        let prlimit_at = got.iter().position(|a| a == "prlimit").unwrap();
        let bwrap_at = got.iter().position(|a| a == "bwrap").unwrap();
        assert!(prlimit_at < bwrap_at);
    }

    #[test]
    fn the_system_paths_a_decoder_needs_are_read_only() {
        let got = wrap(&inner(), Path::new("/in/a.mp4"), Path::new("/out"));
        let joined = got.join(" ");
        assert!(joined.contains("--ro-bind /usr /usr"));
        assert!(joined.contains("--proc /proc"));
        assert!(joined.contains("--dev /dev"));
    }

    // Runs the production argv for real, so the number below is the one the kernel enforced and not the one the argv asked for; bwrap and prlimit are hard runtime dependencies here.
    fn sandboxed_output(inner: &[&str], input: &Path) -> String {
        let inner: Vec<String> = inner.iter().map(|s| s.to_string()).collect();
        let full = wrap_readonly(&inner, input);
        let out = std::process::Command::new(&full[0]).args(&full[1..]).output()
            .unwrap_or_else(|e| panic!("the sandbox wrapper {} could not run: {}", full[0], e));
        assert!(out.status.success(), "the sandboxed prober exited {}: {}", out.status, String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    // Sample line: "VmPeakKb=2116600"
    fn kib(text: &str, key: &str) -> u64 {
        let line = text.lines().find(|l| l.starts_with(key)).unwrap_or_else(|| panic!("the prober printed no {} in: {}", key, text));
        line[key.len()..].trim().parse().unwrap_or_else(|_| panic!("{} is not a number in: {}", key, text))
    }

    #[test]
    fn a_real_sandboxed_child_is_held_to_two_gibibytes_of_address_space() {
        // /etc is bound read-only already, so binding a file inside it is the production shape and nothing more.
        let got = sandboxed_output(&[PYTHON, "-c", RESERVE_PROBE], Path::new("/etc/hostname"));
        assert!(got.contains("cap=2147483648"), "the kernel enforced another cap: {}", got);
        assert!(got.contains("under=ok"), "a 1536 MiB sparse reservation must fit under the cap: {}", got);
        assert!(got.contains("over=refused"), "the cap must still refuse 3072 MiB: {}", got);
        let peak = kib(&got, "VmPeakKb=");
        let rss = kib(&got, "VmRSSKb=");
        assert!(peak > KIB_PER_GIB, "the peak never passed the old one-gibibyte cap, VmPeak {} kB", peak);
        assert!(peak < ADDRESS_SPACE_BYTES / BYTES_PER_KIB, "VmPeak {} kB passed the cap", peak);
        // A cap on address space is not a memory limit, and this is the measurement that says so.
        assert!(rss < RESIDENT_CEILING_KIB, "the sparse reservation became resident, VmRSS {} kB", rss);
    }

    #[test]
    fn a_hostile_path_stays_exactly_one_argument() {
        let hostile = "/in/a; rm -rf ~/b.mp4";
        let got = wrap(&inner(), Path::new(hostile), Path::new("/out"));
        assert_eq!(got.iter().filter(|a| a.as_str() == hostile).count(), 2);
        assert!(!got.iter().any(|a| is_a_shell(a)));
    }
}
