use flea::bytename::ExactText as _;
use std::path::Path;

// bwrap costs a few ms over a bare exec here and systemd-run costs 23.8 ms; see AGENTS.md "Thumbnail sandbox".
pub(crate) const BWRAP: &str = "bwrap";
// bwrap has no rlimit option, so stock prlimit carries them; see AGENTS.md "Thumbnail sandbox".
pub(crate) const PRLIMIT: &str = "prlimit";
// A 1080p decode is well under a second of CPU here, so 30 s is a runaway, not a slow file.
pub(crate) const CPU_SECONDS: u32 = 30;
// Issue #17 reports glycin exhausting 1 GiB of address space on a large ICC-tagged JPEG and aborting, which this box does not reproduce, so the cap is 2 GiB: the smallest value the ticket records as working, still finite, and virtual rather than resident. What actually consumed it is the arena reservation capped above, not the image.
pub(crate) const ADDRESS_SPACE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
// #357: f3d's llvmpipe/LLVM mappings exceed 2 GiB with RSS well below it; the ticket's working cap is 3 GiB.
const F3D_ADDRESS_SPACE_BYTES: u64 = 3 * 1024 * 1024 * 1024;
// Full-font Office conversion peaked at 3,477,811,200 bytes on minipc; 8 GiB holds more than twice that and stays finite.
pub(crate) const OFFICE_ADDRESS_SPACE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
// wrap_readonly_extra leads with prlimit, its CPU flag, its address-space flag and bwrap.
const READONLY_PREFIX_ARGS: usize = 4;
// One read-only bind takes three arguments: the flag, the source and the destination.
const READONLY_BIND_ARGS: usize = 3;

// /bin, /sbin, /lib and /lib64 are all symlinks into usr on this box, so binding /usr covers them.
pub(crate) const BWRAP_FLAGS: &[&str] = &[
    "--unshare-all",
    "--die-with-parent",
    "--new-session",
    "--clearenv",
    // #156: C.UTF-8, so bsdtar passes UTF-8 member names through; after --clearenv or it is wiped.
    "--setenv",
    "LC_ALL",
    "C.UTF-8",
    // Threaded tools reserve whole arenas per thread, so MALLOC_ARENA_MAX=2 keeps them mappable under the 2 GiB cap (issue #17).
    "--setenv",
    "MALLOC_ARENA_MAX",
    "2",
    "--ro-bind",
    "/usr",
    "/usr",
    "--ro-bind",
    "/etc",
    "/etc",
    "--symlink",
    "usr/lib",
    "/lib",
    // Preserve the host loader directory: usr/lib on Arch, usr/lib64 on Debian.
    "--ro-bind-try",
    "/lib64",
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

// bwrap's status fd, kept by bwrap and closed in the sandboxed child, so only bwrap writes it.
pub(crate) const STATUS_FD: i32 = 7;

pub fn available() -> bool {
    available_on(&std::env::var("PATH").unwrap_or_default())
}

// Split from available() so a test can probe the rule; sample input: "/usr/local/bin:/usr/bin:/bin".
fn available_on(path: &str) -> bool {
    let has = |prog: &str| {
        path.split(':')
            .filter(|d| Path::new(d).is_absolute())
            .any(|d| super::jail::executable_file(&Path::new(d).join(prog)))
    };
    has(BWRAP) && has(PRLIMIT)
}

// The decoder's wrapper; the CPU cap is its runaway bound (see AGENTS.md "Thumbnail sandbox").
pub fn wrap(inner: &[String], input: &Path, out: &Path) -> Vec<String> {
    let address_space = match inner.first().and_then(|p| Path::new(p).file_name()) {
        Some(name) if name == "f3d" => F3D_ADDRESS_SPACE_BYTES,
        _ => ADDRESS_SPACE_BYTES,
    };
    wrap_with(inner, input, out, Some(CPU_SECONDS), address_space)
}

// Issue #211: the archive jail for extract and compress, same boundary and address-space cap but no CPU cap.
pub fn wrap_archive(inner: &[String], input: &Path, out: &Path) -> Vec<String> {
    wrap_with(inner, input, out, None, ADDRESS_SPACE_BYTES)
}

// The shared boundary with explicit CPU and finite address-space limits.
pub(crate) fn wrap_with(
    inner: &[String],
    input: &Path,
    out: &Path,
    cpu_seconds: Option<u32>,
    address_space: u64,
) -> Vec<String> {
    let head_and_binds = 10;
    let mut a: Vec<String> = Vec::with_capacity(inner.len() + BWRAP_FLAGS.len() + head_and_binds);
    // prlimit stays outermost so the address-space cap still arrives.
    a.push(PRLIMIT.to_string());
    if let Some(seconds) = cpu_seconds {
        a.push(format!("--cpu={}", seconds));
    }
    a.push(format!("--as={}", address_space));
    a.push(BWRAP.to_string());
    for flag in BWRAP_FLAGS {
        a.push(flag.to_string());
    }
    a.push("--ro-bind".to_string());
    a.push(input.exact_text().to_string());
    a.push(input.exact_text().to_string());
    // The caller names the one writable path, and production binds the single pre-created temp file; see AGENTS.md "Thumbnail sandbox".
    a.push("--bind".to_string());
    a.push(out.exact_text().to_string());
    a.push(out.exact_text().to_string());
    a.extend_from_slice(inner);
    a
}

// The office jail: no CPU cap (LibreOffice's threads outspend 30 CPU seconds inside the 60 s wall clock) and the system font cache read-only, so no conversion rescans the fonts.
pub(crate) fn wrap_office(inner: &[String], input: &Path, out: &Path) -> Vec<String> {
    let mut a = wrap_with(inner, input, out, None, OFFICE_ADDRESS_SPACE_BYTES);
    let at = a.len() - inner.len();
    a.splice(at..at, ["--ro-bind-try", FONT_CACHE, FONT_CACHE].map(String::from));
    a
}
const FONT_CACHE: &str = "/var/cache/fontconfig";

// The pool's namespace flags around the long-lived thumbnail worker, which gets each job's files as descriptors and binds only its own executable; no prlimit, because the limits are per job and a CPU cap would also accumulate the worker's own time across the session.
pub fn wrap_worker(inner: &[String], exe: &Path) -> Vec<String> {
    let head_and_binds = 4;
    let mut a: Vec<String> = Vec::with_capacity(inner.len() + BWRAP_FLAGS.len() + head_and_binds);
    a.push(BWRAP.to_string());
    for flag in BWRAP_FLAGS {
        a.push(flag.to_string());
    }
    a.push("--ro-bind".to_string());
    a.push(exe.exact_text().to_string());
    a.push(exe.exact_text().to_string());
    a.extend_from_slice(inner);
    a
}

// The same boundary with nothing writable, for a probe answering on stdout: ffprobe parses untrusted media too.
pub fn wrap_readonly(inner: &[String], input: &Path) -> Vec<String> {
    wrap_readonly_extra(inner, &[input])
}
pub use super::flashpipe::wrap_stream; // the flasher's stream jail; assembled beside its caller.
// The same boundary with caller-chosen read-only binds and nothing writable, including a figure vendor tree or engine outside /usr.
pub fn wrap_readonly_extra(inner: &[String], ro_binds: &[&Path]) -> Vec<String> {
    wrap_extra(inner, ro_binds, None)
}

// The figure compile jail: the readonly boundary plus one writable directory, the cache's own scratch for this build.
pub fn wrap_compile(inner: &[String], ro_binds: &[&Path], writable: &Path) -> Vec<String> {
    wrap_extra(inner, ro_binds, Some(writable))
}

pub use super::htmlsandbox::wrap_html;

fn wrap_extra(inner: &[String], ro_binds: &[&Path], writable: Option<&Path>) -> Vec<String> {
    let head_and_binds = READONLY_PREFIX_ARGS + ro_binds.len() * READONLY_BIND_ARGS + usize::from(writable.is_some()) * READONLY_BIND_ARGS;
    let mut a: Vec<String> = Vec::with_capacity(inner.len() + BWRAP_FLAGS.len() + head_and_binds);
    a.push(PRLIMIT.to_string());
    a.push(format!("--cpu={}", CPU_SECONDS));
    a.push(format!("--as={}", ADDRESS_SPACE_BYTES));
    a.push(BWRAP.to_string());
    for flag in BWRAP_FLAGS {
        a.push(flag.to_string());
    }
    for bind in ro_binds {
        a.push("--ro-bind".to_string());
        a.push(bind.exact_text().to_string());
        a.push(bind.exact_text().to_string());
    }
    if let Some(dir) = writable {
        a.push("--bind".to_string());
        a.push(dir.exact_text().to_string());
        a.push(dir.exact_text().to_string());
    }
    a.extend_from_slice(inner);
    a
}

// Sample input: full ending in ["/usr/bin/sleep","30"], inner_len 2 inserts before those two.
pub fn add_status(argv: &mut Vec<String>, inner_len: usize) {
    let at = argv.len().saturating_sub(inner_len.min(argv.len()));
    argv.insert(at, "--json-status-fd".to_string());
    argv.insert(at + 1, STATUS_FD.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    // Written out rather than derived from the constant: a test that recomputes the value it checks cannot fail when that value is wrong.
    const TWO_GIB: &str = "--as=2147483648";
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

    // The capacity arithmetic must name the argv's real shape, or the named counts could drift from what the function pushes.
    #[test]
    fn the_readonly_argv_length_matches_its_named_counts() {
        let binds = [Path::new("/in/a.mp4"), Path::new("/in/b.mp4")];
        let got = wrap_readonly_extra(&inner(), &binds);
        let named = READONLY_PREFIX_ARGS + BWRAP_FLAGS.len() + binds.len() * READONLY_BIND_ARGS + inner().len();
        assert_eq!(got.len(), named);
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

    // Issue #211: one jail carries the decoder CPU cap and the other must not; this is the argv itself.
    #[test]
    fn the_archive_jail_drops_the_decoder_cpu_cap_and_keeps_the_memory_cap() {
        let inner = inner();
        let archive = wrap_archive(&inner, Path::new("/in/a.mp4"), Path::new("/out"));
        assert!(!archive.iter().any(|a| a.starts_with("--cpu=")),
                "the archive jail must not carry the decoder's CPU cap: {:?}", archive);
        assert_eq!(archive[0], "prlimit", "the address-space cap still needs prlimit outermost");
        assert!(archive.iter().any(|a| a == TWO_GIB), "and that cap is still 2 GiB: {:?}", archive);
        let joined = archive.join(" ");
        assert!(joined.contains("--ro-bind /in/a.mp4 /in/a.mp4"), "the input is still read-only: {}", joined);
        assert!(joined.contains("--bind /out /out"), "and the named path is still the only writable one");
        assert!(archive.iter().any(|a| a == "--unshare-all"), "the namespace flags are unchanged: {:?}", archive);
        let tail = &archive[archive.len() - inner.len()..];
        assert_eq!(tail, inner.as_slice(), "the inner argv is still last and unchanged");
        // The decoder's own wrapper is untouched by this, because it is the one that needs a runaway bound.
        assert!(wrap(&inner, Path::new("/in/a.mp4"), Path::new("/out")).iter().any(|a| a == "--cpu=30"));
    }

    #[test]
    fn the_system_paths_a_decoder_needs_are_read_only() {
        let got = wrap(&inner(), Path::new("/in/a.mp4"), Path::new("/out"));
        let joined = got.join(" ");
        assert!(joined.contains("--ro-bind /usr /usr"));
        assert!(joined.contains("--proc /proc"));
        assert!(joined.contains("--dev /dev"));
    }

    // #156: the pin is present and survives --clearenv, in both wrappers.
    #[test]
    fn the_jail_pins_a_utf8_locale_after_clearing_the_environment() {
        let got = wrap(&inner(), Path::new("/in/a.mp4"), Path::new("/out"));
        let cleared = got.iter().position(|a| a == "--clearenv").expect("--clearenv");
        let locale = got.iter().position(|a| a == "LC_ALL").expect("LC_ALL");
        assert_eq!(got[locale + 1], "C.UTF-8");
        assert!(cleared < locale, "--clearenv must precede the locale or it clears it too");
        let readonly = wrap_readonly(&inner(), Path::new("/in/a.mp4"));
        assert!(readonly.windows(2).any(|w| w[0] == "LC_ALL" && w[1] == "C.UTF-8"));
    }

    #[test]
    fn a_hostile_path_stays_exactly_one_argument() {
        let hostile = "/in/a; rm -rf ~/b.mp4";
        let got = wrap(&inner(), Path::new(hostile), Path::new("/out"));
        assert_eq!(got.iter().filter(|a| a.as_str() == hostile).count(), 2);
        assert!(!got.iter().any(|a| is_a_shell(a)));
    }
}

#[cfg(test)]
#[path = "sandbox_memory_tests.rs"]
mod memory_tests;

#[cfg(test)]
#[path = "sandbox_f3d_tests.rs"]
mod f3d_tests;
