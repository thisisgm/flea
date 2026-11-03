//! The existing backend owns sandboxed external thumbnailers and RAW extraction.
use super::Request;
use std::{
    io::Read,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use flea::backend::rastertemp::{discard_in, preview_dir, MAX_OUTPUT_BYTES};

// The helper's own thumbnailer stops at 20 s, so this deadline only ends a launcher hung around it.
const DEADLINE: Duration = Duration::from_secs(25);
const POLL: Duration = Duration::from_millis(10);
// A killed group closes the pipe at once; output later than this is dropped with its reader.
const READER_GRACE: Duration = Duration::from_secs(1);
const SIGKILL: i32 = 9;
extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

pub(super) fn run(request: &Request) -> Result<Vec<u8>, ()> {
    let mut command = Command::new(crate::window::startup::binary());
    command
        .arg("--preview-raster")
        .arg(&request.path)
        .arg(request.size.0.max(request.size.1).to_string());
    collect(command, &request.current, DEADLINE, &preview_dir())
}

/// Runs one helper in its own process group and reads its PNG; a retired or overdue helper's group is killed.
fn collect(
    mut command: Command,
    current: &AtomicBool,
    deadline: Duration,
    temps: &Path,
) -> Result<Vec<u8>, ()> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|_| ())?;
    let mut output = child.stdout.take().ok_or(())?;
    let (send, receive) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read = output
            .by_ref()
            .take(MAX_OUTPUT_BYTES as u64 + 1)
            .read_to_end(&mut bytes);
        let _ = send.send((read, bytes));
    });
    let start = Instant::now();
    let result = loop {
        if !current.load(Ordering::Acquire) || start.elapsed() > deadline {
            // SAFETY: this still-owned child's group was created above; kill it before reaping the leader.
            unsafe {
                kill(-(child.id() as i32), SIGKILL);
            }
            let _ = child.wait();
            break Err(());
        }
        match child.try_wait() {
            Ok(Some(status)) => break if status.success() { Ok(()) } else { Err(()) },
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(());
            }
            Ok(None) => std::thread::sleep(POLL),
        }
    };
    // A helper that did not exit cleanly skipped its own temp cleanup; pids are issued in sequence, so none is reused yet.
    if result.is_err() {
        discard_in(temps, child.id());
    }
    let output = receive.recv_timeout(READER_GRACE).map_err(|_| ());
    if output.is_ok() {
        let _ = reader.join();
    }
    result?;
    let (read, bytes) = output?;
    read.map_err(|_| ())?;
    (!bytes.is_empty() && bytes.len() <= MAX_OUTPUT_BYTES)
        .then_some(bytes)
        .ok_or(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview_quality::tests::Root;
    use std::sync::Arc;

    fn names(root: &Root) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(&root.0)
            .unwrap()
            .flatten()
            .map(|item| item.file_name().to_string_lossy().into_owned())
            .filter(|name| name != ".flea-test-sandbox")
            .collect();
        names.sort();
        names
    }

    #[test]
    fn preview_quality_killed_helper_temps_are_discarded_by_pid() {
        let root = Root::new();
        std::fs::write(root.0.join(".flea-1-other.png"), b"x").unwrap();
        // A stand-in helper writes a temp under its own pid, then hangs as a stuck thumbnailer does.
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(r#"printf x > "$0/.flea-$$-temp.png"; exec sleep 60"#)
            .arg(&root.0);
        let current = Arc::new(AtomicBool::new(true));
        let run = {
            let (current, temps) = (current.clone(), root.0.clone());
            std::thread::spawn(move || collect(command, &current, DEADLINE, &temps))
        };
        let start = Instant::now();
        while !names(&root).iter().any(|name| name.ends_with("-temp.png")) {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the helper wrote its temp"
            );
            std::thread::sleep(POLL);
        }
        current.store(false, Ordering::Release);
        assert!(
            run.join().unwrap().is_err(),
            "a retired helper gives no picture"
        );
        assert_eq!(
            names(&root),
            [".flea-1-other.png"],
            "the killed helper's temp is gone and another pid's stays"
        );
    }
}
