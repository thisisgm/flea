use crate::backend::{regfile, sandbox};
use crate::oflags::O_NOFOLLOW;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc,
};
use std::time::{Duration, Instant};

const OUTPUT_LIMIT: u64 = 32 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(10);
pub struct Job {
    cancel: Arc<AtomicBool>,
    pub result: Receiver<Result<Vec<u8>, String>>,
}
impl Job {
    pub fn start(path: PathBuf, mut arguments: Vec<String>) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let (tx, result) = mpsc::channel();
        std::thread::spawn(move || {
            let result = run(&path, &mut arguments, &stopped).map_err(|e| crate::error::io_message(&e));
            let _ = tx.send(result);
        });
        Self { cancel, result }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn run(path: &Path, arguments: &mut [String], cancel: &AtomicBool) -> io::Result<Vec<u8>> {
    if !sandbox::available() {
        return Err(io::Error::other("Preview sandbox is unavailable"));
    }
    let before = std::fs::symlink_metadata(path)?;
    let input = regfile::open_if_regular(path, O_NOFOLLOW)?;
    let after = input.metadata()?;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(io::Error::other("Selected item changed"));
    }
    for argument in arguments.iter_mut() {
        if argument == "{input}" {
            *argument = "/input".into();
        }
    }
    // Pass the held file through stdin: the sandbox cannot inspect its parent's protected /proc descriptors.
    let mut wrapped = sandbox::wrap_readonly(arguments, Path::new("/proc/self/fd/0"));
    let destination = wrapped.len() - arguments.len() - 1;
    wrapped[destination] = "/input".into();
    let mut child = Command::new(&wrapped[0])
        .args(&wrapped[1..])
        .stdin(Stdio::from(input))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Preview output unavailable"))?;
    let overflow = Arc::new(AtomicBool::new(false));
    let exceeded = overflow.clone();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        output.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > OUTPUT_LIMIT {
            exceeded.store(true, Ordering::Relaxed);
        }
        Ok::<_, io::Error>(bytes)
    });
    let started = Instant::now();
    let status = loop {
        if cancel.load(Ordering::Relaxed)
            || overflow.load(Ordering::Relaxed)
            || started.elapsed() > DEADLINE
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("Preview cancelled or exceeded its limit"));
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = reader
        .join()
        .map_err(|_| io::Error::other("Preview reader stopped"))??;
    if !status.success() || bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(io::Error::other("Preview decoder failed"));
    }
    Ok(bytes)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn preview_worker_reads_held_source_in_readonly_sandbox() {
        if crate::backend::sandboxprobe::skipped() { return; }
        let root = TestDir::new("tui-preview-held");
        let path = root.file("source.txt", "held preview source\n");
        let job = Job::start(path.clone(), vec![
            "/usr/bin/bash".into(), "-c".into(),
            "test ! -w /input && /usr/bin/cat /input".into(),
        ]);
        let result = job.result.recv_timeout(Duration::from_secs(5)).expect("preview worker did not finish");
        assert_eq!(result.unwrap(), b"held preview source\n");
        assert_eq!(std::fs::read(path).unwrap(), b"held preview source\n");
    }

    #[test]
    fn missing_preview_source_reports_plain_worker_error() {
        if crate::backend::sandboxprobe::skipped() { return; }
        let root = TestDir::new("tui-preview-error");
        let job = Job::start(root.join("missing.pdf"), vec!["/usr/bin/pdfinfo".into(), "{input}".into()]);
        let result = job.result.recv_timeout(Duration::from_secs(5)).expect("preview worker did not finish");
        assert_eq!(result.unwrap_err(), "file or folder not found");
    }
}
