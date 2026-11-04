use crate::backend::{regfile, sandbox};
use crate::oflags::O_NOFOLLOW;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc,
};
use std::time::{Duration, Instant};

const OUTPUT_LIMIT: u64 = 32 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(10);
// Only held() constructs this read-only regular-file capability.
pub(super) struct HeldSource(std::fs::File);
impl HeldSource {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        held(path).map(Self)
    }
    pub(super) fn metadata(&self) -> io::Result<std::fs::Metadata> {
        self.0.metadata()
    }
}
pub struct Job {
    cancel: Arc<AtomicBool>,
    pub result: Receiver<Result<Vec<u8>, String>>,
    fault: Arc<std::sync::Mutex<Option<&'static str>>>,
}
impl Job {
    pub fn start(path: PathBuf, mut arguments: Vec<String>) -> Self {
        Self::spawn(move |cancel| run(&path, &mut arguments, cancel))
    }
    pub(super) fn start_held(input: &HeldSource, mut arguments: Vec<String>) -> io::Result<Self> {
        let input = input.0.try_clone()?;
        Ok(Self::spawn(move |cancel| {
            require_sandbox()?;
            run_held(input, &mut arguments, cancel)
        }))
    }
    fn spawn(work: impl FnOnce(&AtomicBool) -> io::Result<Vec<u8>> + Send + 'static) -> Self {
        let cancel = Arc::new(AtomicBool::new(false));
        let stopped = cancel.clone();
        let (tx, result) = mpsc::channel();
        let fault = Arc::new(std::sync::Mutex::new(None));
        let reported = fault.clone();
        std::thread::spawn(move || {
            let result = work(&stopped).map_err(|error| {
                if let Some(fault) = error
                    .get_ref()
                    .and_then(|cause| cause.downcast_ref::<super::preview::words::Fault>())
                {
                    if let Ok(mut value) = reported.lock() {
                        *value = Some(fault.key);
                    }
                }
                crate::error::io_message(&error)
            });
            let _ = tx.send(result);
        });
        Self {
            cancel,
            result,
            fault,
        }
    }
}
impl Job {
    pub fn failure(&self, diagnostic: String) -> super::preview::words::Text {
        self.fault
            .lock()
            .ok()
            .and_then(|key| *key)
            .map(super::preview::words::Text::Key)
            .unwrap_or(super::preview::words::Text::Data(diagnostic))
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn require_sandbox() -> io::Result<()> {
    if sandbox::available() {
        Ok(())
    } else {
        Err(super::preview::words::issue(
            "actions.preview.no_sandbox",
            "Preview sandbox is unavailable",
        ))
    }
}
fn run(path: &Path, arguments: &mut [String], cancel: &AtomicBool) -> io::Result<Vec<u8>> {
    require_sandbox()?;
    run_held(held(path)?, arguments, cancel)
}
fn run_held(
    input: std::fs::File,
    arguments: &mut [String],
    cancel: &AtomicBool,
) -> io::Result<Vec<u8>> {
    for argument in arguments.iter_mut() {
        if argument == "{input}" {
            *argument = "/input".into();
        }
    }
    // Bwrap consumes the read-only descriptor while mounting /input, before decoder exec.
    let mut wrapped = sandbox::wrap_readonly(arguments, Path::new("/proc/self/fd/0"));
    let destination = wrapped.len() - arguments.len() - 1;
    wrapped[destination - 2] = "--ro-bind-fd".into();
    wrapped[destination - 1] = "0".into();
    wrapped[destination] = "/input".into();
    sandbox::add_status(&mut wrapped, arguments.len());
    #[cfg(test)]
    super::chafa::test_support::host_loader(&mut wrapped);
    let mut jailed = crate::backend::jail::spawn_jailed(&wrapped, |cmd| {
        cmd.stdin(Stdio::from(input));
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());
    })?;
    let output = jailed.child.stdout.take().ok_or_else(|| {
        super::preview::words::issue("actions.preview.no_output", "Preview output unavailable")
    })?;
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
            // Ends the sandbox init too, and never waits for the output drain.
            crate::backend::jail::kill_tree(&mut jailed);
            return Err(super::preview::words::issue(
                "tui.preview.worker_limit",
                "Preview cancelled or exceeded its limit",
            ));
        }
        if let Some(status) = jailed.child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = reader.join().map_err(|_| {
        super::preview::words::issue("tui.preview.reader_stopped", "Preview reader stopped")
    })??;
    decoded(status, bytes)
}
// The regular file at path, opened without following a link and refused if it was swapped between the look and the open.
pub fn held(path: &Path) -> io::Result<std::fs::File> {
    let before = std::fs::symlink_metadata(path)?;
    let input = regfile::open_if_regular(path, O_NOFOLLOW)?;
    let after = input.metadata()?;
    if before.dev() != after.dev() || before.ino() != after.ino() {
        return Err(super::preview::words::issue(
            "tui.preview.changed",
            "Selected item changed",
        ));
    }
    Ok(input)
}
// The decoder's output when it exited cleanly inside the limit, and a plain refusal otherwise.
fn decoded(status: std::process::ExitStatus, bytes: Vec<u8>) -> io::Result<Vec<u8>> {
    if !status.success() || bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(super::preview::words::issue(
            "preview.unshown",
            "Preview could not be shown",
        ));
    }
    Ok(bytes)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::super::chafa::test_support::sandboxed;
    use super::*;
    use crate::backend::testdir::TestDir;

    #[test]
    fn no_process_can_write_chmod_truncate_or_touch_the_held_host_file() {
        use std::os::unix::fs::PermissionsExt;
        if !sandboxed() {
            return;
        }
        let root = TestDir::new("preview-host-mutations");
        let path = root.file("image", "held preview source\n");
        root.assert_contains(&path);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        let script = r#"
bad=0; matches=0
[ "$(/usr/bin/cat /input)" = 'held preview source' ] || exit 1
for path in /proc/[0-9]*/fd/* /proc/[0-9]*/map_files/* /proc/[0-9]*/cwd /proc/[0-9]*/root/input; do
    [ -e "$path" ] || continue
    if [ "$path" -ef /input ]; then
        matches=$((matches+1))
        if /bin/sh -c 'printf rewritten > "$1"' sh "$path" 2>/dev/null; then bad=$((bad+1)); fi
        if chmod 0666 "$path" 2>/dev/null; then bad=$((bad+1)); fi
        if /usr/bin/truncate -s 0 "$path" 2>/dev/null; then bad=$((bad+1)); fi
        if /usr/bin/touch -d 2000-01-01 "$path" 2>/dev/null; then bad=$((bad+1)); fi
    fi
done
# Every match attempts all four mutations, so a run with no match would prove nothing.
[ "$matches" -gt 0 ] || exit 1
printf 'held preview source\n'
exit "$bad"
"#;
        for _ in 0..3 {
            let job = Job::start(
                path.clone(),
                vec!["/bin/sh".into(), "-c".into(), script.into()],
            );
            let result = job.result.recv_timeout(Duration::from_secs(5)).unwrap();
            let after = std::fs::metadata(&path).unwrap();
            assert_eq!(
                (
                    std::fs::read(&path).unwrap(),
                    after.permissions().mode() & 0o777,
                    after.modified().unwrap(),
                ),
                (b"held preview source\n".to_vec(), 0o644, before),
            );
            assert_eq!(result.unwrap(), b"held preview source\n");
        }
    }

    #[test]
    fn preview_worker_reads_held_source_in_readonly_sandbox() {
        if !sandboxed() {
            return;
        }
        let root = TestDir::new("tui-preview-held");
        let path = root.file("source.txt", "held preview source\n");
        let job = Job::start(
            path.clone(),
            vec![
                "/usr/bin/bash".into(),
                "-c".into(),
                "test ! -w /input && /usr/bin/cat /input".into(),
            ],
        );
        let result = job
            .result
            .recv_timeout(Duration::from_secs(5))
            .expect("preview worker did not finish");
        assert_eq!(result.unwrap(), b"held preview source\n");
        assert_eq!(std::fs::read(path).unwrap(), b"held preview source\n");
    }

    #[test]
    fn a_decoder_that_exits_nonzero_or_overflows_reports_the_preview_as_not_shown() {
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            decoded(std::process::ExitStatus::from_raw(0), b"ok".to_vec()).unwrap(),
            b"ok"
        );
        let failed = decoded(std::process::ExitStatus::from_raw(3 << 8), Vec::new()).unwrap_err();
        assert_eq!(
            crate::error::io_message(&failed),
            "Preview could not be shown"
        );
        let overflow = vec![0; OUTPUT_LIMIT as usize + 1];
        assert!(decoded(std::process::ExitStatus::from_raw(0), overflow).is_err());
    }

    #[test]
    fn missing_preview_source_reports_plain_worker_error() {
        if !sandboxed() {
            return;
        }
        let root = TestDir::new("tui-preview-error");
        let job = Job::start(
            root.join("missing.pdf"),
            vec!["/usr/bin/pdfinfo".into(), "{input}".into()],
        );
        let result = job
            .result
            .recv_timeout(Duration::from_secs(5))
            .expect("preview worker did not finish");
        assert_eq!(result.unwrap_err(), "item not found");
    }
}

#[cfg(test)]
impl Job {
    // A job whose worker is already gone: its result channel reads Disconnected.
    pub fn orphaned() -> Self {
        let (_, result) = mpsc::channel();
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            result,
            fault: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}
