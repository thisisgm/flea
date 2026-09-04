use crate::backend::archivelist::{clear_counts, parse_until, Contents, ARCHIVE_READ_MS};
use crate::backend::archivespec::ListSpec;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SIGKILL: i32 = 9;

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

pub fn run(argv: &[String], spec: &ListSpec) -> Contents {
    run_with_budget(argv, spec, Duration::from_millis(ARCHIVE_READ_MS))
}

fn run_with_budget(argv: &[String], spec: &ListSpec, budget: Duration) -> Contents {
    let failed = || Contents { failed: true, ..Default::default() };
    let mut child = match Command::new(&argv[0]).args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).process_group(0).spawn() {
        Ok(child) => child,
        Err(_) => return failed(),
    };
    let done = Arc::new(AtomicBool::new(false));
    let watchdog = watchdog(Arc::clone(&done), child.id() as i32, budget);
    let mut contents = match child.stdout.take() {
        Some(output) => parse_until(std::io::BufReader::new(output), spec, budget),
        None => {
            done.store(true, Ordering::Relaxed);
            let _ = watchdog.join();
            let _ = child.kill();
            let _ = child.wait();
            return failed();
        }
    };
    if contents.failed {
        unsafe { kill(-(child.id() as i32), SIGKILL) };
    }
    done.store(true, Ordering::Relaxed);
    let _ = watchdog.join();
    if !matches!(child.wait(), Ok(status) if status.success()) {
        contents.failed = true;
    }
    if contents.failed {
        clear_counts(&mut contents);
    }
    contents
}

fn watchdog(done: Arc<AtomicBool>, pid: i32, budget: Duration) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let deadline = Instant::now() + budget;
        while Instant::now() < deadline {
            if done.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10).min(deadline.saturating_duration_since(Instant::now())));
        }
        if !done.load(Ordering::Relaxed) {
            unsafe { kill(-pid, SIGKILL) };
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::archivespec::tar_spec;
    use crate::backend::testdir::TestDir;

    const LINE: &str = "-rw-r--r-- 0 u g 2 Sep 1 12:10 ./a.txt";

    #[test]
    fn archive_output_is_streamed_and_final_status_is_authoritative() {
        let many = format!("yes -- '{}' | head -n 5000", LINE);
        let argv = ["/usr/bin/sh".to_string(), "-c".to_string(), many];
        let contents = run_with_budget(&argv, &tar_spec(), Duration::from_secs(2));
        assert!(!contents.failed);
        assert_eq!(contents.entries, 5000);

        let nonzero = format!("printf '%s\\n' '{}'; exit 7", LINE);
        let argv = ["/usr/bin/sh".to_string(), "-c".to_string(), nonzero];
        let contents = run_with_budget(&argv, &tar_spec(), Duration::from_secs(2));
        assert!(contents.failed);
        assert_eq!((contents.entries, contents.unpacked, contents.produced_entries), (0, 0, 0));
    }

    #[test]
    fn archive_watchdog_kills_the_full_tree_and_clears_partial_counts() {
        let dir = TestDir::new("archive-reader-tree");
        let pid_file = dir.join("pid");
        let script = format!("printf '%s\\n' '{}'; (sleep 60) & echo $! > '{}'; wait", LINE, pid_file.display());
        let argv = ["/usr/bin/sh".to_string(), "-c".to_string(), script];
        let started = Instant::now();
        let contents = run_with_budget(&argv, &tar_spec(), Duration::from_millis(150));
        assert!(contents.failed);
        assert_eq!((contents.entries, contents.unpacked, contents.produced_entries), (0, 0, 0));
        assert!(started.elapsed() < Duration::from_secs(2));
        let pid = std::fs::read_to_string(pid_file).unwrap();
        let proc_path = std::path::Path::new("/proc").join(pid.trim());
        for _ in 0..100 {
            if !proc_path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(!proc_path.exists(), "archive descendant survived watchdog cleanup");
    }
}
