// The flea crate's backend/sandboxprobe.rs check, for the pv3 helper binaries that cannot reach its cfg(test) module.
use std::io::Write;
use std::process::Command;
use std::sync::OnceLock;

// Probe the kernel boundary independently of the helper's sandbox argument builder.
pub fn skipped() -> bool {
    static FAILURE: OnceLock<Option<String>> = OnceLock::new();
    let failure = FAILURE.get_or_init(|| {
        match Command::new("bwrap")
            .args([
                "--unshare-user",
                "--unshare-pid",
                "--unshare-net",
                "--die-with-parent",
                "--ro-bind",
                "/",
                "/",
                "--proc",
                "/proc",
                "/usr/bin/true",
            ])
            .output()
        {
            Ok(output) if output.status.success() => None,
            Ok(output) => Some(
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .next()
                    .unwrap_or("it exited non-zero and said nothing")
                    .to_owned(),
            ),
            Err(error) => Some(error.to_string()),
        }
    });
    let Some(reason) = failure else { return false };
    let thread = std::thread::current();
    let name = thread.name().unwrap_or("pv3 helper test");
    let _ = writeln!(
        std::io::stderr(),
        "SKIP {name}: the bwrap sandbox cannot run here: {reason}"
    );
    true
}
