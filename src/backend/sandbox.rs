use std::path::Path;

// bwrap costs a few ms over a bare exec here and systemd-run costs 23.8 ms; see AGENTS.md "Thumbnail sandbox".
const BWRAP: &str = "bwrap";
// bwrap has no rlimit option, so stock prlimit carries them; see AGENTS.md "Thumbnail sandbox".
const PRLIMIT: &str = "prlimit";
// A 1080p decode is well under a second of CPU here, so 30 s is a runaway, not a slow file.
const CPU_SECONDS: u32 = 30;
// ffmpegthumbnailer peaks in the tens of megabytes on this fixture, so 1 GiB is a decompression bomb, not a big video.
const ADDRESS_SPACE_BYTES: u64 = 1024 * 1024 * 1024;

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

    // The jail is mandatory, so the one input that decides it is asserted rather than assumed: an
    // empty PATH must read unavailable, or every caller's fail-closed branch is unreachable.
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
        assert_eq!(got[0], "prlimit");
        assert!(got.iter().any(|a| a.starts_with("--cpu=")));
        assert!(got.iter().any(|a| a.starts_with("--as=")));
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

    #[test]
    fn a_hostile_path_stays_exactly_one_argument() {
        let hostile = "/in/a; rm -rf ~/b.mp4";
        let got = wrap(&inner(), Path::new(hostile), Path::new("/out"));
        assert_eq!(got.iter().filter(|a| a.as_str() == hostile).count(), 2);
        assert!(!got.iter().any(|a| is_a_shell(a)));
    }
}
