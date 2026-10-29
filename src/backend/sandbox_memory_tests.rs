use super::*;
use std::path::Path;

// The kernel's own spelling of 2 GiB, written out so neither spelling can drift unseen.
const TWO_GIB_TEXT: &str = "2147483648";
// /proc reports VmPeak and VmRSS in kibibytes, and the reservations below are sized in mebibytes.
const KIB_PER_GIB: u64 = 1024 * 1024;
const BYTES_PER_KIB: u64 = 1024;
// 256 MiB, twenty times the prober's measured resident size: a reservation must not become memory.
const RESIDENT_CEILING_KIB: u64 = 262_144;
// The one interpreter on this box that can ask the kernel for a mapping of a chosen protection.
const PYTHON: &str = "/usr/bin/python3";

// Sample row: "Max cpu time              30                   30                   seconds"; a limit never given reads "unlimited".
const LIMITS_PROBE: &str = r#"
def field(path, prefix, column):
    return next(l.split()[column] for l in open(path) if l.startswith(prefix))
print("cpu=" + field("/proc/self/limits", "Max cpu time", 3))
print("as=" + field("/proc/self/limits", "Max address space", 3))
"#;

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

// Runs the production argv for real; bwrap and prlimit are hard runtime dependencies here.
fn wrapped_output(argv: &[String]) -> String {
    let out = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .unwrap_or_else(|e| panic!("the sandbox wrapper {} could not run: {}", argv[0], e));
    assert!(
        out.status.success(),
        "the sandboxed prober exited {}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn sandboxed_output(inner: &[&str], input: &Path) -> String {
    let inner: Vec<String> = inner.iter().map(|s| s.to_string()).collect();
    wrapped_output(&wrap_readonly(&inner, input))
}

// Sample line: "cpu=unlimited"
fn limits(text: &str, key: &str) -> String {
    let prefix = format!("{}=", key);
    let line = text
        .lines()
        .find(|l| l.starts_with(&prefix))
        .unwrap_or_else(|| panic!("the prober printed no {} in: {}", prefix, text));
    line[prefix.len()..].trim().to_string()
}

// Sample line: "VmPeakKb=2116600"
fn kib(text: &str, key: &str) -> u64 {
    let line = text
        .lines()
        .find(|l| l.starts_with(key))
        .unwrap_or_else(|| panic!("the prober printed no {} in: {}", key, text));
    line[key.len()..]
        .trim()
        .parse()
        .unwrap_or_else(|_| panic!("{} is not a number in: {}", key, text))
}

#[test]
fn a_real_sandboxed_child_is_held_to_two_gibibytes_of_address_space() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    // /etc is bound read-only already, so binding a file inside it is the production shape and nothing more.
    let got = sandboxed_output(&[PYTHON, "-c", RESERVE_PROBE], Path::new("/etc/hostname"));
    assert!(
        got.contains("cap=2147483648"),
        "the kernel enforced another cap: {}",
        got
    );
    assert!(
        got.contains("under=ok"),
        "a 1536 MiB sparse reservation must fit under the cap: {}",
        got
    );
    assert!(
        got.contains("over=refused"),
        "the cap must still refuse 3072 MiB: {}",
        got
    );
    let peak = kib(&got, "VmPeakKb=");
    let rss = kib(&got, "VmRSSKb=");
    assert!(
        peak > KIB_PER_GIB,
        "the peak never passed the old one-gibibyte cap, VmPeak {} kB",
        peak
    );
    assert!(
        peak < ADDRESS_SPACE_BYTES / BYTES_PER_KIB,
        "VmPeak {} kB passed the cap",
        peak
    );
    // A cap on address space is not a memory limit, and this is the measurement that says so.
    assert!(
        rss < RESIDENT_CEILING_KIB,
        "the sparse reservation became resident, VmRSS {} kB",
        rss
    );
}

// Issue #211 at the enforced level: the decoder jail holds 30 CPU seconds and the archive jail none.
#[test]
fn the_archive_jail_has_no_cpu_limit_where_the_decoder_one_has_thirty_seconds() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let d = crate::backend::testdir::TestDir::new("sandboxlimits");
    let input = Path::new("/etc/hostname");
    let inner: Vec<String> = [PYTHON, "-c", LIMITS_PROBE]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let decoder = wrapped_output(&wrap(&inner, input, d.path()));
    let archive = wrapped_output(&wrap_archive(&inner, input, d.path()));
    assert_eq!(
        limits(&decoder, "cpu"),
        "30",
        "the decoder jail lost its runaway bound: {}",
        decoder
    );
    // "unlimited" is the kernel's word for no RLIMIT_CPU, so an extract outlives the 30 s that killed it.
    assert_eq!(
        limits(&archive, "cpu"),
        "unlimited",
        "the archive jail still carries a CPU cap: {}",
        archive
    );
    for (who, text) in [("decoder", &decoder), ("archive", &archive)] {
        assert_eq!(
            limits(text, "as"),
            TWO_GIB_TEXT,
            "the {} jail must keep the address-space cap: {}",
            who,
            text
        );
    }
}

#[test]
fn only_the_f3d_program_gets_three_gib_and_other_jails_keep_two() {
    for program in [
        "/usr/bin/f3d",
        "/opt/f3d/bin/f3d",
        "/usr/bin/glycin-thumbnailer",
        "/usr/bin/ffmpegthumbnailer",
        "/opt/not-f3d",
    ] {
        let inner = vec![program.to_string(), "f3d".to_string()];
        let argv = wrap(&inner, Path::new("/in"), Path::new("/out"));
        let want = if program.ends_with("/f3d") {
            "--as=3221225472"
        } else {
            "--as=2147483648"
        };
        assert_eq!(&argv[..4], &["prlimit", "--cpu=30", want, "bwrap"]);
        for other in [
            wrap_archive(&inner, Path::new("/in"), Path::new("/out")),
            wrap_readonly(&inner, Path::new("/in")),
        ] {
            assert!(other.iter().any(|a| a == "--as=2147483648"));
        }
    }
    assert!(wrap(&[], Path::new("/in"), Path::new("/out"))
        .iter()
        .any(|a| a == "--as=2147483648"));
}

// Each concurrent jail must allow sparse reservations and refuse a forked allocation bomb.
#[test]
fn concurrent_f3d_jails_allow_reservations_but_stop_an_inherited_memory_bomb() {
    if crate::backend::sandboxprobe::skipped() {
        return;
    }
    let dir = crate::backend::testdir::TestDir::new("f3d-memory");
    let program = dir.join("f3d");
    std::os::unix::fs::symlink(PYTHON, &program).unwrap();
    let script = r#"
import ctypes, os, resource
libc = ctypes.CDLL(None, use_errno=True)
libc.mmap.restype = ctypes.c_void_p
libc.mmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_long]
assert resource.getrlimit(resource.RLIMIT_AS) == (3221225472, 3221225472)
assert resource.getrlimit(resource.RLIMIT_CPU) == (30, 30)
p = libc.mmap(None, 2560*1024*1024, 0, 0x02 | 0x20 | 0x4000, -1, 0)
assert p != ctypes.c_void_p(-1).value, 'a sparse reservation must fit'
libc.munmap.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
assert libc.munmap(p, 2560*1024*1024) == 0
child = os.fork()
if child == 0:
    try:
        bomb = bytearray(4*1024*1024*1024)
    except MemoryError:
        os._exit(0)
    os._exit(1)
assert os.waitpid(child, 0)[1] == 0, 'the inherited cap must stop the bomb'
rss = next(int(l.split()[1]) for l in open('/proc/self/status') if l.startswith('VmRSS:'))
assert rss < 262144, rss
print('reservation=ok bomb=stopped rss_kib=' + str(rss))
"#;
    std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..3)
            .map(|i| {
                let output = dir.file(&format!("out-{i}"), "");
                let inner = vec![
                    program.to_string_lossy().into_owned(),
                    "-c".to_string(),
                    script.to_string(),
                ];
                let argv = wrap(&inner, &program, &output);
                scope.spawn(move || {
                    let text = wrapped_output(&argv);
                    assert!(text.contains("reservation=ok bomb=stopped"), "{text}");
                    eprintln!("{text}");
                })
            })
            .collect();
        for job in jobs {
            job.join().unwrap();
        }
    });
}
