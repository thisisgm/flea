// A PDF on a hung mount must not freeze the window: the fetch runs beside the loop under a stall deadline into a session-private copy the viewer draws.
use crate::backend::opsreq::OpMsg;
use crate::json::escape;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, LazyLock, Mutex, Once};
use std::time::Duration;

// How long one fetch waits for the filesystem without progress before the viewer says the file is not responding.
pub const PDF_COPY_WAIT: Duration = Duration::from_secs(8);
// One directory per backend process under the shared leaf, so no process ever sweeps another's live copy.
const PDF_CACHE_LEAF: &str = "flea/pdf";
static SWEEP: Once = Once::new();
// One copy on screen per viewer slot, so a newer fetch supersedes only its own viewer.
static LAST_COPY: LazyLock<Mutex<HashMap<String, PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
// The per-slot table, made once because a static cannot call HashMap::new.
fn last_copy() -> &'static Mutex<HashMap<String, PathBuf>> {
    &LAST_COPY
}

fn cache_root() -> PathBuf {
    crate::userfile::cache_home().join(PDF_CACHE_LEAF)
}

// Sample input: {"c":"pdfcopy","id":3,"slot":"column","path":"/run/media/gm/128GB/doc.pdf"}.
pub fn pdfcopied_line(id: usize, path: &str, err: &str, stalled: bool) -> String {
    if err.is_empty() {
        format!(r#"{{"t":"pdfcopied","id":{},"path":"{}"}}"#, id, escape(path))
    } else if stalled {
        format!(r#"{{"t":"pdfcopied","id":{},"err":"{}","stalled":true}}"#, id, escape(err))
    } else {
        format!(r#"{{"t":"pdfcopied","id":{},"err":"{}"}}"#, id, escape(err))
    }
}

fn dest_for(dir: &Path, id: usize, src: &Path) -> PathBuf {
    let stem = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document.pdf".to_string());
    dir.join(format!("{id}-{stem}"))
}

// A pid with no /proc entry is gone, so its directory is litter and not a live copy.
fn sweep_dead(root: &Path, live: u32) {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for item in entries.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if name == live.to_string() || !name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if std::fs::metadata(format!("/proc/{name}")).is_err() {
            let _ = std::fs::remove_dir_all(item.path());
        }
    }
}

// A chunk copied is progress, so the wait restarts on every one and bounds a stall, not the file.
enum CopyMsg {
    Chunk,
    Done(Result<(), String>),
}

// The copy itself, chunk by chunk so a cancel or a stall deadline stops paying for bytes nobody reads.
fn copy_into(src: &Path, dst: &Path, done: &Arc<AtomicBool>, progress: &std::sync::mpsc::Sender<CopyMsg>) -> Result<(), String> {
    let mut reader = std::fs::File::open(src).map_err(|e| crate::error::io_message(&e))?;
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| crate::error::io_message(&e))?;
    }
    let mut writer = std::fs::File::create(dst).map_err(|e| crate::error::io_message(&e))?;
    let mut buf = vec![0u8; 262144];
    loop {
        if done.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(dst);
            return Err("cancelled".to_string());
        }
        use std::io::Read;
        let n = reader.read(&mut buf).map_err(|e| crate::error::io_message(&e))?;
        if n == 0 {
            break;
        }
        use std::io::Write;
        writer.write_all(&buf[..n]).map_err(|e| crate::error::io_message(&e))?;
        let _ = progress.send(CopyMsg::Chunk);
    }
    Ok(())
}

// The wait loop with its receive as a seam, so a test drives progress without sleeping.
fn await_copy(recv: &mut dyn FnMut(Duration) -> Result<CopyMsg, RecvTimeoutError>, id: usize, dst: &Path, tx: &std::sync::mpsc::Sender<OpMsg>, wait: Duration, done: &AtomicBool) {
    loop {
        match recv(wait) {
            Ok(CopyMsg::Chunk) => {}
            Ok(CopyMsg::Done(Ok(()))) => {
                let _ = tx.send(OpMsg::Meta { line: pdfcopied_line(id, &dst.to_string_lossy(), "", false) });
                break;
            }
            Ok(CopyMsg::Done(Err(msg))) => {
                let _ = std::fs::remove_file(dst);
                let _ = tx.send(OpMsg::Meta { line: pdfcopied_line(id, "", &msg, false) });
                break;
            }
            Err(_) => {
                done.store(true, Ordering::Relaxed);
                let _ = tx.send(OpMsg::Meta { line: pdfcopied_line(id, "", "that file is not responding", true) });
                break;
            }
        }
    }
}

fn fetch(id: usize, slot: &str, src: PathBuf, tx: std::sync::mpsc::Sender<OpMsg>, wait: Duration, root: &Path) {
    let me = std::process::id();
    SWEEP.call_once(|| sweep_dead(root, me));
    let dst = dest_for(&root.join(me.to_string()), id, &src);
    if let Ok(mut last) = last_copy().lock() {
        if let Some(prev) = last.insert(slot.to_string(), dst.clone()) {
            if prev != dst {
                let _ = std::fs::remove_file(&prev);
            }
        }
    }
    let done = Arc::new(AtomicBool::new(false));
    let (one_tx, one_rx) = channel::<CopyMsg>();
    let worker_done = Arc::clone(&done);
    let worker_dst = dst.clone();
    std::thread::spawn(move || {
        let result = copy_into(&src, &worker_dst, &worker_done, &one_tx);
        if !worker_done.load(Ordering::Relaxed) {
            let _ = one_tx.send(CopyMsg::Done(result));
        }
    });
    let mut recv = |ask: Duration| one_rx.recv_timeout(ask);
    await_copy(&mut recv, id, &dst, &tx, wait, &done);
}

pub fn spawn(id: usize, slot: String, path: PathBuf, tx: std::sync::mpsc::Sender<OpMsg>) {
    std::thread::spawn(move || fetch(id, &slot, path, tx, PDF_COPY_WAIT, &cache_root()));
}

#[cfg(test)]
mod tests {
    use super::*;

    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn a_local_file_is_handed_back_as_a_private_copy() {
        let _held = SERIAL.lock().unwrap();
        let d = crate::backend::testdir::TestDir::new("pdfcopy-local");
        let src = d.file("doc.pdf", "%PDF-1.4 local\n");
        let root = d.dir("pdf");
        let (tx, rx) = channel::<OpMsg>();
        fetch(1, "column", src, tx, Duration::from_secs(10), &root);
        let line = match rx.recv_timeout(Duration::from_secs(10)).expect("fetch always answers") {
            OpMsg::Meta { line } => line,
            _ => panic!("a copy answers on the meta line"),
        };
        let me = std::process::id().to_string();
        assert!(line.contains(&format!("{me}/1-doc.pdf")), "the copy lives under this process's own directory: {line}");
        let dst = root.join(format!("{me}/1-doc.pdf"));
        assert_eq!(std::fs::read(&dst).unwrap(), b"%PDF-1.4 local\n");
    }

    #[test]
    fn a_newer_fetch_supersedes_the_copy_on_screen() {
        let _held = SERIAL.lock().unwrap();
        let d = crate::backend::testdir::TestDir::new("pdfcopy-supersede");
        let first = d.file("a.pdf", "first");
        let second = d.file("b.pdf", "second");
        let root = d.dir("pdf");
        let me = std::process::id().to_string();
        let (tx, rx) = channel::<OpMsg>();
        fetch(1, "column", first, tx, Duration::from_secs(10), &root);
        assert!(matches!(rx.recv_timeout(Duration::from_secs(10)), Ok(OpMsg::Meta { .. })));
        let (tx, rx) = channel::<OpMsg>();
        fetch(2, "column", second, tx, Duration::from_secs(10), &root);
        assert!(matches!(rx.recv_timeout(Duration::from_secs(10)), Ok(OpMsg::Meta { .. })));
        assert!(root.join(format!("{me}/2-b.pdf")).is_file(), "the newer copy stays");
        assert!(!root.join(format!("{me}/1-a.pdf")).is_file(), "the superseded copy is gone");
    }

    #[test]
    fn a_viewer_never_removes_another_viewers_copy() {
        let _held = SERIAL.lock().unwrap();
        let d = crate::backend::testdir::TestDir::new("pdfcopy-isolation");
        let first = d.file("a.pdf", "first");
        let second = d.file("b.pdf", "second");
        let root = d.dir("pdf");
        let me = std::process::id().to_string();
        let (tx, rx) = channel::<OpMsg>();
        fetch(2, "column", first, tx, Duration::from_secs(10), &root);
        assert!(matches!(rx.recv_timeout(Duration::from_secs(10)), Ok(OpMsg::Meta { .. })));
        let (tx, rx) = channel::<OpMsg>();
        fetch(1, "quicklook", second, tx, Duration::from_secs(10), &root);
        assert!(matches!(rx.recv_timeout(Duration::from_secs(10)), Ok(OpMsg::Meta { .. })));
        assert!(root.join(format!("{me}/2-a.pdf")).is_file(), "the other viewer's copy stays");
        assert!(root.join(format!("{me}/1-b.pdf")).is_file(), "the newer copy stays");
    }

    #[test]
    fn the_sweep_keeps_live_processes_and_drops_dead_ones() {
        let d = crate::backend::testdir::TestDir::new("pdfcopy-sweep");
        let root = d.dir("pdf");
        // A reaped child lends a pid nothing else holds, so the swept directory is truly dead.
        let mut child = std::process::Command::new("sleep").arg("30").spawn().expect("a reaped child leaves a dead pid");
        let dead = child.id();
        child.kill().ok();
        let _ = child.wait();
        assert!(std::fs::metadata(format!("/proc/{dead}")).is_err(), "the reaped child is really gone");
        let me = std::process::id().to_string();
        std::fs::create_dir_all(root.join(&me)).unwrap();
        std::fs::write(root.join(&me).join("copy.pdf"), "live").unwrap();
        std::fs::create_dir_all(root.join("notes")).unwrap();
        std::fs::create_dir_all(root.join(dead.to_string())).unwrap();
        std::fs::write(root.join(dead.to_string()).join("copy.pdf"), "dead").unwrap();
        sweep_dead(&root, me.parse().unwrap());
        assert!(root.join(&me).join("copy.pdf").is_file(), "this process's directory stays");
        assert!(root.join("notes").is_dir(), "a non-pid directory stays");
        assert!(!root.join(dead.to_string()).exists(), "a dead process's directory is swept");
    }

    #[test]
    fn a_reply_line_names_the_copy_or_the_wait() {
        assert_eq!(pdfcopied_line(3, "/x/doc.pdf", "", false), r#"{"t":"pdfcopied","id":3,"path":"/x/doc.pdf"}"#);
        assert_eq!(pdfcopied_line(3, "", "permission denied", false), r#"{"t":"pdfcopied","id":3,"err":"permission denied"}"#);
        assert_eq!(pdfcopied_line(3, "", "that file is not responding", true), r#"{"t":"pdfcopied","id":3,"err":"that file is not responding","stalled":true}"#);
    }

    #[test]
    fn a_hung_source_answers_not_responding_through_fetch() {
        let _held = SERIAL.lock().unwrap();
        let d = crate::backend::testdir::TestDir::new("pdfcopy-hang");
        let fifo = d.join("hung.pdf");
        crate::backend::fifotest::mkfifo(&fifo);
        // Read-write opens a fifo without blocking, so the copy's own open succeeds and its read blocks.
        let _writer = std::fs::OpenOptions::new().read(true).write(true).open(&fifo).unwrap();
        let (tx, rx) = channel::<OpMsg>();
        fetch(7, "column", fifo, tx, Duration::from_millis(500), &d.dir("pdf"));
        let line = match rx.recv_timeout(Duration::from_secs(10)).expect("fetch always answers") {
            OpMsg::Meta { line } => line,
            _ => panic!("a hung source answers on the meta line"),
        };
        assert!(line.contains(r#""id":7"#), "the answer names the fetch it hung: {line}");
        assert!(line.contains("that file is not responding"), "fetch itself answers the wait: {line}");
        assert!(line.contains(r#""stalled":true"#), "the wait that ran out carries the machine flag: {line}");
    }

    #[test]
    fn every_chunk_reports_progress_so_a_slow_read_is_not_a_stall() {
        let d = crate::backend::testdir::TestDir::new("pdfcopy-progress");
        let src = d.file("big.pdf", &"x".repeat(600 * 1024));
        let dst = d.join("copy.pdf");
        let done = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel::<CopyMsg>();
        copy_into(&src, &dst, &done, &tx).expect("a local file copies");
        let mut chunks = 0;
        while let Ok(msg) = rx.try_recv() {
            if matches!(msg, CopyMsg::Chunk) {
                chunks += 1;
            }
        }
        assert_eq!(chunks, 3, "each 256 KiB chunk restarts the wait: {chunks}");
    }

    #[test]
    fn progress_restarts_the_wait_on_every_chunk() {
        let d = crate::backend::testdir::TestDir::new("pdfcopy-waitseam");
        let dst = d.join("copy.pdf");
        let wait = Duration::from_secs(8);
        let script = std::cell::RefCell::new(vec![Ok(CopyMsg::Chunk), Ok(CopyMsg::Chunk), Ok(CopyMsg::Done(Ok(())))]);
        let seen = std::cell::RefCell::new(Vec::new());
        let mut recv = |ask: Duration| {
            seen.borrow_mut().push(ask);
            script.borrow_mut().remove(0)
        };
        let (tx, rx) = channel::<OpMsg>();
        let done = AtomicBool::new(false);
        await_copy(&mut recv, 9, &dst, &tx, wait, &done);
        assert_eq!(*seen.borrow(), vec![wait, wait, wait], "each call gets the full wait: {:?}", *seen.borrow());
        let line = match rx.recv_timeout(Duration::from_secs(5)).expect("the wait answers") {
            OpMsg::Meta { line } => line,
            _ => panic!("the wait answers on the meta line"),
        };
        assert!(line.contains(r#""id":9"#), "the answer names the fetch: {line}");
        assert!(!line.contains("not responding"), "progress is not a stall: {line}");
    }
}
