use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const SERVICE: &str = "xdg-desktop-portal.service";
const FRONTEND: &str = "org.freedesktop.portal.Desktop";
const OBJECT: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.FileChooser";

// A stopped frontend stays stopped, and a live one must export FileChooser before success is reported.
pub fn restart() -> bool {
    let mut restart = Command::new("systemctl");
    restart.args(["--user", "try-restart", SERVICE]);
    let restarted =
        bounded(restart, Duration::from_secs(15)).is_some_and(|out| out.status.success());
    if restarted {
        let mut probe = Command::new("busctl");
        probe.args([
            "--user",
            "--auto-start=no",
            "--timeout=5s",
            "get-property",
            FRONTEND,
            OBJECT,
            IFACE,
            "version",
        ]);
        if bounded(probe, Duration::from_secs(6))
            .is_some_and(|out| out.status.success() && ready_reply(&out.stdout))
        {
            return true;
        }
    }
    eprintln!("flea: the portal restart could not be verified. Run: systemctl --user restart xdg-desktop-portal; then: journalctl --user -u xdg-desktop-portal.service -b --no-pager");
    false
}

fn ready_reply(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut words = text.split_whitespace();
    words.next() == Some("u")
        && words
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .is_some_and(|version| version > 0)
        && words.next().is_none()
}

// systemctl otherwise waits through the unit's 90-second activation timeout when a backend stalls.
fn bounded(mut command: Command, limit: Duration) -> Option<Output> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frontend_must_answer_a_nonzero_unsigned_filechooser_version() {
        assert!(ready_reply(b"u 4\n"));
        assert!(ready_reply(b"u 1\n"));
        for refused in [
            b"".as_slice(),
            b"u 0\n",
            b"s 4\n",
            b"u -1\n",
            b"u 4294967296\n",
            b"u 4 trailing",
            b"\xff",
        ] {
            assert!(!ready_reply(refused), "refused: {:?}", refused);
        }
    }

    #[test]
    fn a_stalled_helper_is_killed_and_reaped_at_the_bound() {
        let mut command = Command::new("/bin/sleep");
        command.arg("10");
        let start = Instant::now();
        assert!(bounded(command, Duration::from_millis(50)).is_none());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
