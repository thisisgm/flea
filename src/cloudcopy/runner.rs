use super::emit;
use crate::jsondoc::{self, Json};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};
extern "C" {
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
    fn getppid() -> i32;
}
const MAX_OUTPUT: usize = 32 * 1024 * 1024;

pub fn owner_lifetime() -> Result<(), String> {
    let parent = unsafe { getppid() };
    if parent == 1 || unsafe { prctl(1, 9, 0, 0, 0) } != 0 || unsafe { getppid() } != parent {
        return Err("Upload owner is unavailable".into());
    }
    Ok(())
}
pub struct Runner {
    pub cancel: Arc<AtomicBool>,
    pub deadline: Instant,
}
impl Runner {
    pub fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err("Cancelled; originals kept. Uploaded files may remain.".into());
        }
        if Instant::now() > self.deadline {
            return Err(
                "Upload deadline reached; originals kept. Retry to continue by file.".into(),
            );
        }
        Ok(())
    }
    pub fn run(&self, args: &[String], state: &str) -> Result<String, String> {
        self.check()?;
        let mut command = Command::new("rclone");
        command
            .args(args)
            .args([
                "--retries",
                "2",
                "--low-level-retries",
                "3",
                "--contimeout",
                "15s",
                "--timeout",
                "60s",
                "--use-json-log",
                "--stats",
                "1s",
                "--stats-log-level",
                "NOTICE",
                "--log-level",
                "NOTICE",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Environment flags must not silently turn a copy into filtering, link following or deletion.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("RCLONE_")
                && key != "RCLONE_CONFIG"
                && !key.to_string_lossy().starts_with("RCLONE_CONFIG_")
            {
                command.env_remove(key);
            }
        }
        let parent = std::process::id() as i32;
        unsafe {
            command.pre_exec(move || {
                if prctl(1, 9, 0, 0, 0) != 0 || getppid() != parent {
                    return Err(std::io::Error::other("upload owner exited"));
                }
                Ok(())
            });
        }
        let mut child = command
            .spawn()
            .map_err(|_| "rclone could not start. Install it and configure the selected remote.")?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::sync_channel(16);
        let reader = std::thread::spawn(move || {
            let mut output = Vec::new();
            let result = stdout
                .take((MAX_OUTPUT + 1) as u64)
                .read_to_end(&mut output);
            (result, output)
        });
        let logs = std::thread::spawn(move || {
            let mut input = BufReader::new(stderr);
            loop {
                let mut line = Vec::new();
                match input.by_ref().take(65537).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() > 65536 => break,
                    Ok(_) => {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let mut failed = None;
        let status = loop {
            while let Ok(line) = rx.try_recv() {
                if let Ok(value) = jsondoc::parse(&String::from_utf8_lossy(&line)) {
                    if let Some(stats) = value.get("stats") {
                        let number = |key| {
                            stats
                                .get(key)
                                .and_then(Json::as_f64)
                                .filter(|x| x.is_finite() && *x >= 0.0)
                                .unwrap_or(0.0)
                        };
                        emit(
                            state,
                            "",
                            number("bytes"),
                            number("totalBytes"),
                            number("speed"),
                        );
                    }
                }
            }
            if let Err(error) = self.check() {
                failed = Some(error);
                let _ = child.kill();
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(_) => {
                    failed = Some("Could not wait for rclone".into());
                    let _ = child.kill();
                    break child.wait().map_err(|_| "Could not reap rclone")?;
                }
            }
        };
        drop(rx);
        let _ = logs.join();
        let (read, output) = reader
            .join()
            .map_err(|_| "Could not collect rclone output")?;
        if let Some(error) = failed {
            return Err(error);
        }
        read.map_err(|_| "Could not read rclone output")?;
        if output.len() > MAX_OUTPUT {
            return Err("rclone output exceeded the safety limit".into());
        }
        if !status.success() {
            return Err(if state == "verifying" { "Checksum verification failed, the destination contains a conflicting/partial file, or MD5 is unsupported. Originals kept. Choose an unused destination to retry." }
                else { "rclone failed (connection, permission or destination conflict). Originals kept; retry is safe for matching files." }.into());
        }
        String::from_utf8(output).map_err(|_| "rclone returned non-text output".into())
    }
}
