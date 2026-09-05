// Dispatch for the five write operations: one runs at a time, because the status bar has one sticky slot for it.
use crate::backend::ops;
use crate::backend::opsreq::{
    duplicated_line, made_line, op_err, renamed_line, run_duplicate, run_transfer, run_trash, trashed_line,
    transferdone_line, transferitem_line, transferprogress_line, transferstarted_line, undone_line, usable_dest,
    OpMsg,
};
use crate::backend::listing::Listing;
use crate::backend::proto::error_line;
use crate::backend::undo::{Entry, Journal};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;

// Everything the write operations own, kept apart from the listing state they never touch.
pub(crate) struct Ops {
    pub journal: Journal,
    pub next_id: usize,
    // The id of the operation on the thread, or None when none is running; the cap is one at a time.
    pub running: Option<usize>,
    pub cancel: Arc<AtomicBool>,
    pub tx: Sender<OpMsg>,
}

impl Ops {
    pub fn new(tx: Sender<OpMsg>) -> Ops {
        Ops { journal: Journal::new(), next_id: 1, running: None, cancel: Arc::new(AtomicBool::new(false)), tx }
    }

    // An id with no slot claimed: archive and convert are id-keyed and run concurrently by design,
    // so they number themselves without taking the transfer's one-at-a-time slot.
    pub fn claim_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    // A fresh flag per operation, so a cancel can never reach the operation after the one it was aimed at.
    fn claim(&mut self) -> (usize, Arc<AtomicBool>) {
        let id = self.next_id;
        self.next_id += 1;
        self.running = Some(id);
        self.cancel = Arc::new(AtomicBool::new(false));
        (id, Arc::clone(&self.cancel))
    }
}

// The status bar shows one operation, so a second one is refused as data rather than queued invisibly behind the first.
fn busy(out: &mut impl Write, where_: &str) -> bool {
    let e = op_err(where_, "", "an operation is already running");
    writeln!(out, "{}", error_line(&e)).ok();
    out.flush().ok();
    true
}

// Explicit paths win; a rows form is resolved against the listing here, at request time, so the
// operation still owns a snapshot that outlives whatever the listing does next.
pub(crate) fn resolve_rows(paths: Vec<String>, rows: &[usize], base: &Path, listing: &Listing) -> Vec<String> {
    if !paths.is_empty() {
        return paths;
    }
    rows.iter()
        .filter(|&&r| r < listing.len())
        .map(|&r| base.join(listing.name(r)).to_string_lossy().to_string())
        .collect()
}

pub(crate) fn start_transfer(out: &mut impl Write, ops: &mut Ops, op: &str, paths: Vec<String>, dest: &str) {
    if ops.running.is_some() {
        busy(out, "transfer");
        return;
    }
    let dest = match usable_dest(dest) {
        Ok(d) => d,
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
            out.flush().ok();
            return;
        }
    };
    // Anything that is not exactly "move" is a copy, so a malformed op can never remove a source.
    let moving = op == "move";
    let n = paths.len();
    let (id, cancel) = ops.claim();
    writeln!(out, "{}", transferstarted_line(id, n, moving)).ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || run_transfer(id, moving, paths, dest, cancel, tx));
}

// No response line of its own: the running operation answers with its own terminal transferdone.
pub(crate) fn cancel_transfer(ops: &Ops, id: usize) {
    if ops.running == Some(id) {
        ops.cancel.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn start_trash(out: &mut impl Write, ops: &mut Ops, paths: Vec<String>) {
    if ops.running.is_some() {
        busy(out, "trash");
        return;
    }
    ops.claim();
    let tx = ops.tx.clone();
    thread::spawn(move || run_trash(paths, tx));
}

pub(crate) fn start_duplicate(out: &mut impl Write, ops: &mut Ops, path: &str) {
    if ops.running.is_some() {
        busy(out, "duplicate");
        return;
    }
    ops.claim();
    let tx = ops.tx.clone();
    let owned = path.to_string();
    thread::spawn(move || run_duplicate(owned, tx));
}

// Rename answers on the calling thread; rclone directory compatibility may copy before removing its source.
pub(crate) fn do_rename(out: &mut impl Write, ops: &mut Ops, path: &str, to: &str) {
    match ops::rename(Path::new(path), to) {
        Ok((dst, steps)) => {
            ops.journal.push(Entry { op: "rename".to_string(), steps });
            writeln!(out, "{}", renamed_line(true, &dst.to_string_lossy())).ok();
        }
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
        }
    }
    out.flush().ok();
}

// One mkdir(2), so like rename it answers on the calling thread and never takes the operation slot.
pub(crate) fn do_mkdir(out: &mut impl Write, ops: &mut Ops, parent: &str, name: &str) {
    match ops::mkdir(Path::new(parent), name) {
        Ok((dir, steps)) => {
            ops.journal.push(Entry { op: "mkdir".to_string(), steps });
            writeln!(out, "{}", made_line(true, &dir.to_string_lossy())).ok();
        }
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
        }
    }
    out.flush().ok();
}

pub(crate) fn do_undo(out: &mut impl Write, ops: &mut Ops) {
    match ops.journal.undo() {
        Ok(op) => writeln!(out, "{}", undone_line(&op, true)).ok(),
        Err(e) => writeln!(out, "{}", error_line(&e)).ok(),
    };
    out.flush().ok();
}

// Every message an operation thread sends, written out and, when terminal, recorded in the journal.
pub(crate) fn report_op(out: &mut impl Write, ops: &mut Ops, msg: OpMsg) {
    match msg {
        OpMsg::Progress { id, index, name, bytes, total } => {
            writeln!(out, "{}", transferprogress_line(id, index, &name, bytes, total)).ok();
        }
        OpMsg::Item { id, index, name, ok, err } => {
            writeln!(out, "{}", transferitem_line(id, index, &name, ok, &err)).ok();
        }
        OpMsg::TransferDone { id, ok, failed, skipped, cancelled, entry } => {
            ops.journal.push(entry);
            ops.running = None;
            writeln!(out, "{}", transferdone_line(id, ok, failed, skipped, cancelled)).ok();
        }
        OpMsg::Trashed { ok, failed, entry } => {
            ops.journal.push(entry);
            ops.running = None;
            writeln!(out, "{}", trashed_line(ok, failed)).ok();
        }
        // Meta never claims the operation slot, so it does not clear it either.
        OpMsg::Meta { line } => {
            writeln!(out, "{}", line).ok();
        }
        OpMsg::Duplicated { ok, path, err, entry } => {
            ops.journal.push(entry);
            ops.running = None;
            if ok {
                writeln!(out, "{}", duplicated_line(true, &path)).ok();
            } else {
                writeln!(out, "{}", error_line(&op_err("duplicate", "", &err))).ok();
            }
        }
    }
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::backend::undo::Step;
    use std::sync::mpsc::channel;

    fn ops() -> Ops {
        let (tx, _rx) = channel();
        Ops::new(tx)
    }

    fn out() -> Vec<u8> {
        Vec::new()
    }

    fn text(buf: &[u8]) -> String {
        String::from_utf8_lossy(buf).to_string()
    }

    #[test]
    fn each_operation_claims_a_new_id_and_its_own_cancel_flag() {
        let mut o = ops();
        let (first, first_flag) = o.claim();
        o.running = None;
        let (second, second_flag) = o.claim();
        assert_eq!((first, second), (1, 2));
        first_flag.store(true, Ordering::Relaxed);
        assert!(
            !second_flag.load(Ordering::Relaxed),
            "a cancel aimed at the first operation must never reach the one after it"
        );
    }

    #[test]
    fn a_cancel_for_an_operation_that_is_not_running_does_nothing() {
        let mut o = ops();
        let (id, flag) = o.claim();
        cancel_transfer(&o, id + 99);
        assert!(!flag.load(Ordering::Relaxed), "a stale id must not cancel the live operation");
        cancel_transfer(&o, id);
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn a_rename_records_its_reversal_and_undo_puts_the_name_back() {
        let d = TestDir::new("dispatchrename");
        let mut o = ops();
        let from = d.file("before.txt", "body");
        let mut buf = out();
        do_rename(&mut buf, &mut o, &from.to_string_lossy(), "after.txt");
        assert!(d.join("after.txt").exists());
        assert!(text(&buf).contains(r#""t":"renamed","ok":true"#));
        assert_eq!(o.journal.len(), 1);
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains(r#"{"t":"undone","op":"rename","ok":true}"#));
        assert!(from.exists(), "undo put the old name back");
        assert!(o.journal.is_empty());
    }

    // Journal::undo propagates with `?` and do_undo hands that error straight to error_line.
    #[test]
    fn a_failed_rename_reversal_answers_the_rename_kind_and_not_undo() {
        let d = TestDir::new("dispatchundorename");
        let mut o = ops();
        let from = d.file("before.txt", "body");
        let mut buf = out();
        do_rename(&mut buf, &mut o, &from.to_string_lossy(), "after.txt");
        // Something takes the old name back before the undo, so the reversal's own rename refuses it.
        d.file("before.txt", "squatter");
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        let line = text(&buf);
        assert!(line.contains(r#""t":"error","where":"rename""#), "{}", line);
        assert!(!line.contains(r#""where":"undo""#), "an undo must not re-stamp the kind its step answered");
    }

    #[test]
    fn a_refused_rename_records_nothing_to_undo() {
        let d = TestDir::new("dispatchrefuse");
        let mut o = ops();
        let from = d.file("a.txt", "a");
        d.file("b.txt", "b");
        let mut buf = out();
        do_rename(&mut buf, &mut o, &from.to_string_lossy(), "b.txt");
        assert!(text(&buf).contains(r#""t":"error","where":"rename""#), "the refusal is an error line, not a silent no-op");
        assert!(o.journal.is_empty(), "a rename that did not happen must not be undoable");
        assert_eq!(std::fs::read_to_string(d.join("b.txt")).unwrap(), "b");
    }

    #[test]
    fn explicit_paths_win_and_a_rows_form_resolves_against_the_listing() {
        let mut l = Listing::new();
        l.push("sub", true);
        l.push("a.txt", false);
        l.push("b.txt", false);
        let base = Path::new("/home/gm");
        // A wide selection reaches the backend as indices, because the client only holds its own window.
        assert_eq!(
            resolve_rows(Vec::new(), &[1, 2], base, &l),
            vec!["/home/gm/a.txt".to_string(), "/home/gm/b.txt".to_string()]
        );
        // A row past the end is dropped rather than panicking or naming the base directory itself.
        assert_eq!(resolve_rows(Vec::new(), &[99], base, &l), Vec::<String>::new());
        // Explicit paths are never second-guessed against the listing.
        assert_eq!(
            resolve_rows(vec!["/elsewhere/c.txt".to_string()], &[0, 1, 2], base, &l),
            vec!["/elsewhere/c.txt".to_string()]
        );
        assert_eq!(resolve_rows(Vec::new(), &[], base, &l), Vec::<String>::new());
    }

    #[test]
    fn a_second_operation_while_one_runs_is_refused_as_data_rather_than_queued_invisibly() {
        let d = TestDir::new("dispatchbusy");
        let mut o = ops();
        o.claim();
        let mut buf = out();
        start_trash(&mut buf, &mut o, vec![d.file("a.txt", "a").to_string_lossy().to_string()]);
        assert!(text(&buf).contains("an operation is already running"));
        assert!(d.join("a.txt").exists(), "the refused operation touched nothing");
    }

    #[test]
    fn a_terminal_message_clears_the_running_slot_so_the_next_operation_is_accepted() {
        let mut o = ops();
        o.claim();
        assert!(o.running.is_some());
        let mut buf = out();
        report_op(
            &mut buf,
            &mut o,
            OpMsg::Trashed { ok: 1, failed: 0, entry: Entry { op: "trash".to_string(), steps: vec![Step::Created { path: "/x".into() }] } },
        );
        assert!(o.running.is_none(), "the cap would otherwise refuse every operation for the rest of the session");
        assert_eq!(o.journal.len(), 1);
        assert_eq!(text(&buf).trim(), r#"{"t":"trashed","ok":1,"failed":0}"#);
    }

    #[test]
    fn a_new_folder_records_its_reversal_and_undo_removes_it() {
        let d = TestDir::new("dispatchmkdir");
        let mut o = ops();
        let mut buf = out();
        do_mkdir(&mut buf, &mut o, &d.path().to_string_lossy(), "photos");
        assert!(d.join("photos").is_dir());
        assert_eq!(text(&buf).trim(), format!(r#"{{"t":"made","ok":true,"path":"{}"}}"#, d.join("photos").display()));
        assert_eq!(o.journal.len(), 1);
        let mut buf = out();
        do_undo(&mut buf, &mut o);
        assert!(text(&buf).contains(r#"{"t":"undone","op":"mkdir","ok":true}"#));
        assert!(!d.join("photos").exists(), "undo removed the folder it made");
        assert!(o.journal.is_empty());
    }

    #[test]
    fn a_refused_new_folder_records_nothing_to_undo() {
        let d = TestDir::new("dispatchmkdirrefuse");
        let mut o = ops();
        d.file("taken", "t");
        let mut buf = out();
        do_mkdir(&mut buf, &mut o, &d.path().to_string_lossy(), "taken");
        assert!(text(&buf).contains(r#""t":"error","where":"mkdir""#), "the refusal is an error line, not a silent no-op");
        assert!(o.journal.is_empty(), "a folder that was not made must not be undoable");
        assert_eq!(std::fs::read_to_string(d.join("taken")).unwrap(), "t");
    }
}
