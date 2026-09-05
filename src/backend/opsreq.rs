// The operations request layer: the response lines, and the one thread an operation runs on.
use crate::backend::copyfile::{copy_any, move_any, Progress};
use crate::backend::ops;
use crate::backend::trash;
use crate::backend::undo::{Entry, Step};
use crate::error::FleaError;
use crate::json::escape;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

// One progress line per item at most this often, so a fast copy of a small file may emit none at all.
const PROGRESS_EVERY: Duration = Duration::from_millis(150);

// What an operation thread sends back, joined onto the same receiver every other event already arrives on.
pub enum OpMsg {
    Progress { id: usize, index: usize, name: String, bytes: u64, total: u64 },
    Item { id: usize, index: usize, name: String, ok: bool, err: String },
    TransferDone { id: usize, ok: usize, failed: usize, skipped: usize, cancelled: bool, entry: Entry },
    Trashed { ok: usize, failed: usize, entry: Entry },
    Duplicated { ok: bool, path: String, err: String, entry: Entry },
    // Not an operation: meta rides this channel because a media probe is a subprocess and the loop
    // must not wait on one. Nothing about it claims the one-at-a-time slot.
    Meta { line: String },
}

// moving is the verb the request actually resolved to, so the client names the operation from the
// wire rather than from a clipboard it may have already spent or never owned.
pub fn transferstarted_line(id: usize, n: usize, moving: bool) -> String {
    format!(r#"{{"t":"transferstarted","id":{},"n":{},"moving":{}}}"#, id, n, moving)
}

// total is 0 for a directory, whose size is not known in advance without the sweep this codebase never does.
pub fn transferprogress_line(id: usize, index: usize, name: &str, bytes: u64, total: u64) -> String {
    format!(
        r#"{{"t":"transferprogress","id":{},"index":{},"name":"{}","bytes":{},"total":{}}}"#,
        id, index, escape(name), bytes, total
    )
}

// err rides only on a failure, so a successful item's line carries no empty field to reason about.
pub fn transferitem_line(id: usize, index: usize, name: &str, ok: bool, err: &str) -> String {
    if ok {
        return format!(
            r#"{{"t":"transferitem","id":{},"index":{},"name":"{}","ok":true}}"#,
            id, index, escape(name)
        );
    }
    format!(
        r#"{{"t":"transferitem","id":{},"index":{},"name":"{}","ok":false,"err":"{}"}}"#,
        id, index, escape(name), escape(err)
    )
}

pub fn transferdone_line(id: usize, ok: usize, failed: usize, skipped: usize, cancelled: bool) -> String {
    format!(
        r#"{{"t":"transferdone","id":{},"ok":{},"failed":{},"skipped":{},"cancelled":{}}}"#,
        id, ok, failed, skipped, cancelled
    )
}

pub fn trashed_line(ok: usize, failed: usize) -> String {
    format!(r#"{{"t":"trashed","ok":{},"failed":{}}}"#, ok, failed)
}

pub fn renamed_line(ok: bool, path: &str) -> String {
    format!(r#"{{"t":"renamed","ok":{},"path":"{}"}}"#, ok, escape(path))
}

pub fn duplicated_line(ok: bool, path: &str) -> String {
    format!(r#"{{"t":"duplicated","ok":{},"path":"{}"}}"#, ok, escape(path))
}

pub fn made_line(ok: bool, path: &str) -> String {
    format!(r#"{{"t":"made","ok":{},"path":"{}"}}"#, ok, escape(path))
}

pub fn undone_line(op: &str, ok: bool) -> String {
    format!(r#"{{"t":"undone","op":"{}","ok":{}}}"#, escape(op), ok)
}

// A destination Flea will not create as a side effect, checked once before any item is touched.
pub fn usable_dest(dest: &str) -> Result<PathBuf, FleaError> {
    let p = PathBuf::from(dest);
    if !p.is_absolute() {
        return Err(op_err("transfer", dest, "a destination must be an absolute path"));
    }
    match p.metadata() {
        Ok(m) if m.is_dir() => Ok(p),
        Ok(_) => Err(op_err("transfer", dest, "the destination is not a directory")),
        Err(e) => Err(op_err("transfer", dest, &e.to_string())),
    }
}

pub fn op_err(where_: &str, path: &str, msg: &str) -> FleaError {
    FleaError { where_: where_.to_string(), path: path.to_string(), msg: msg.to_string() }
}

fn base_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

// Copy or move, one top-level item at a time, reporting each item's own terminal line as it lands.
pub fn run_transfer(
    id: usize,
    moving: bool,
    paths: Vec<String>,
    dest: PathBuf,
    cancel: Arc<AtomicBool>,
    tx: Sender<OpMsg>,
) {
    let mut steps: Vec<Step> = Vec::new();
    let (mut ok, mut failed, mut skipped) = (0usize, 0usize, 0usize);
    let mut was_cancelled = false;
    for (index, raw) in paths.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            was_cancelled = true;
            skipped += 1;
            continue;
        }
        let src = PathBuf::from(raw);
        let name = base_name(&src);
        let dst = dest.join(&name);
        match one_item(id, index, &name, moving, &src, &dst, &cancel, &tx, &mut steps) {
            Ok(()) => {
                ok += 1;
                let _ = tx.send(OpMsg::Item { id, index, name, ok: true, err: String::new() });
            }
            Err(e) => {
                if e.msg == "cancelled" {
                    was_cancelled = true;
                }
                failed += 1;
                let _ = tx.send(OpMsg::Item { id, index, name, ok: false, err: e.msg });
            }
        }
    }
    let entry = Entry { op: if moving { "move".to_string() } else { "copy".to_string() }, steps };
    let _ = tx.send(OpMsg::TransferDone { id, ok, failed, skipped, cancelled: was_cancelled, entry });
}

// A directory has no total without a sweep, so only a file item reports bytes at all. Its journal
// steps land in `steps` either way: a failure that created its destination left a partial there.
fn one_item(
    id: usize,
    index: usize,
    name: &str,
    moving: bool,
    src: &Path,
    dst: &Path,
    cancel: &AtomicBool,
    tx: &Sender<OpMsg>,
    steps: &mut Vec<Step>,
) -> Result<(), FleaError> {
    let is_file = src.symlink_metadata().map(|m| m.is_file()).unwrap_or(false);
    let mut last = Instant::now() - PROGRESS_EVERY;
    let mut sink = |done: u64, total: u64| {
        if !is_file || last.elapsed() < PROGRESS_EVERY {
            return;
        }
        last = Instant::now();
        let _ = tx.send(OpMsg::Progress {
            id,
            index,
            name: name.to_string(),
            bytes: done,
            total,
        });
    };
    let mut p = Progress { cancel, on_bytes: &mut sink, partial: None };
    let outcome = if moving { move_any(src, dst, &mut p) } else { copy_any(src, dst, &mut p) };
    match &outcome {
        Ok(()) if moving => steps.push(Step::Moved { from: src.to_path_buf(), to: dst.to_path_buf() }),
        Ok(()) => steps.push(Step::Created { path: dst.to_path_buf() }),
        // The partial is this operation's, so it is journaled and undo removes it like any created path.
        Err(_) => {
            if let Some(path) = p.partial.take() {
                steps.push(Step::Created { path });
            }
        }
    }
    outcome
}

pub fn run_trash(paths: Vec<String>, tx: Sender<OpMsg>) {
    let owned: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let (entries, failed) = trash::trash(&owned);
    let ok = entries.len();
    let steps = entries.into_iter().map(Step::Trashed).collect();
    let entry = Entry { op: "trash".to_string(), steps };
    let _ = tx.send(OpMsg::Trashed { ok, failed, entry });
}

pub fn run_duplicate(path: String, tx: Sender<OpMsg>) {
    let (outcome, steps) = ops::duplicate(Path::new(&path));
    // Carried on a failure too: the steps then name the partial copy the failure left behind.
    let entry = Entry { op: "duplicate".to_string(), steps };
    let msg = match outcome {
        Ok(dst) => OpMsg::Duplicated { ok: true, path: dst.to_string_lossy().to_string(), err: String::new(), entry },
        Err(e) => OpMsg::Duplicated { ok: false, path: String::new(), err: e.msg, entry },
    };
    let _ = tx.send(msg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::backend::undo::Journal;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc::{channel, Receiver};

    // Drains an operation's channel to its terminal line, which is what every transfer case reads.
    fn done_line(rx: Receiver<OpMsg>) -> (usize, usize, usize, bool, Entry) {
        let mut done = None;
        for msg in rx.iter() {
            if let OpMsg::TransferDone { ok, failed, skipped, cancelled, entry, .. } = msg {
                done = Some((ok, failed, skipped, cancelled, entry));
            }
        }
        done.expect("a terminal line")
    }

    #[test]
    fn a_successful_item_line_carries_no_err_field_at_all() {
        let line = transferitem_line(12, 0, "a.txt", true, "");
        assert_eq!(line, r#"{"t":"transferitem","id":12,"index":0,"name":"a.txt","ok":true}"#);
        assert!(!line.contains("err"));
    }

    #[test]
    fn a_failed_item_line_carries_its_reason_escaped() {
        let line = transferitem_line(12, 1, "say \"hi\".txt", false, "permission denied");
        assert!(line.contains(r#""ok":false"#));
        assert!(line.contains(r#""err":"permission denied""#));
        assert!(line.contains(r#"say \"hi\".txt"#), "a name is escaped like every other string on this wire");
    }

    #[test]
    fn every_operation_line_matches_the_shape_the_operations_design_names() {
        assert_eq!(transferstarted_line(12, 2, false), r#"{"t":"transferstarted","id":12,"n":2,"moving":false}"#);
        assert_eq!(transferstarted_line(12, 2, true), r#"{"t":"transferstarted","id":12,"n":2,"moving":true}"#);
        assert_eq!(
            transferprogress_line(12, 0, "a.txt", 40000000, 120000000),
            r#"{"t":"transferprogress","id":12,"index":0,"name":"a.txt","bytes":40000000,"total":120000000}"#
        );
        assert_eq!(
            transferdone_line(12, 1, 1, 0, false),
            r#"{"t":"transferdone","id":12,"ok":1,"failed":1,"skipped":0,"cancelled":false}"#
        );
        assert_eq!(trashed_line(1, 0), r#"{"t":"trashed","ok":1,"failed":0}"#);
        assert_eq!(renamed_line(true, "/home/gm/new.txt"), r#"{"t":"renamed","ok":true,"path":"/home/gm/new.txt"}"#);
        assert_eq!(
            duplicated_line(true, "/home/gm/photo copy.jpg"),
            r#"{"t":"duplicated","ok":true,"path":"/home/gm/photo copy.jpg"}"#
        );
        assert_eq!(made_line(true, "/home/gm/New Folder"), r#"{"t":"made","ok":true,"path":"/home/gm/New Folder"}"#);
        assert_eq!(undone_line("move", true), r#"{"t":"undone","op":"move","ok":true}"#);
    }

    #[test]
    fn a_destination_that_is_not_an_existing_directory_is_refused_before_any_item_is_touched() {
        let d = TestDir::new("dest");
        assert!(usable_dest(&d.path().to_string_lossy()).is_ok());
        let file = d.file("not-a-dir.txt", "body");
        assert_eq!(
            usable_dest(&file.to_string_lossy()).unwrap_err().msg,
            "the destination is not a directory"
        );
        assert!(usable_dest("relative/path").is_err(), "a relative destination is never resolved here");
        assert!(usable_dest(&d.join("missing").to_string_lossy()).is_err(), "Flea does not create the destination");
    }

    #[test]
    fn a_copy_transfer_records_only_what_it_created_and_leaves_the_sources() {
        let d = TestDir::new("transfercopy");
        let src = d.file("a.txt", "body");
        let dest = d.dir("out");
        let (tx, rx) = channel();
        run_transfer(1, false, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
        assert!(src.exists(), "a copy leaves its source");
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "body");
        let (ok, failed, _, _, entry) = done_line(rx);
        assert_eq!((ok, failed), (1, 0));
        assert_eq!(entry.op, "copy");
        assert_eq!(entry.steps, vec![Step::Created { path: dest.join("a.txt") }]);
    }

    #[test]
    fn a_move_transfer_records_where_each_item_came_from() {
        let d = TestDir::new("transfermove");
        let src = d.file("b.txt", "body");
        let dest = d.dir("out");
        let (tx, rx) = channel();
        run_transfer(2, true, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
        assert!(!src.exists(), "a move leaves nothing at the source");
        let (_, _, _, _, entry) = done_line(rx);
        assert_eq!(entry.op, "move");
        assert_eq!(entry.steps, vec![Step::Moved { from: src, to: dest.join("b.txt") }]);
    }

    #[test]
    fn one_failing_item_is_data_and_the_batch_carries_on() {
        let d = TestDir::new("transferpartial");
        let good = d.file("good.txt", "body");
        let missing = d.join("never-existed.txt");
        let dest = d.dir("out");
        let (tx, rx) = channel();
        run_transfer(
            3,
            false,
            vec![missing.to_string_lossy().to_string(), good.to_string_lossy().to_string()],
            dest.clone(),
            Arc::new(AtomicBool::new(false)),
            tx,
        );
        assert!(dest.join("good.txt").exists(), "the item after the failure still ran");
        let mut counts = None;
        let mut errs = Vec::new();
        for msg in rx.iter() {
            match msg {
                OpMsg::TransferDone { ok, failed, .. } => counts = Some((ok, failed)),
                OpMsg::Item { ok: false, err, .. } => errs.push(err),
                _ => {}
            }
        }
        assert_eq!(counts, Some((1, 1)));
        assert_eq!(errs.len(), 1, "the failure is one item's data, not the operation's");
    }

    // A file with no permission bits answers EACCES to open(2) for every uid but root, so it forces
    // the failure a real permission error would, in whichever order read_dir yields the tree.
    #[test]
    fn a_copy_that_fails_short_of_a_cancel_records_the_partial_tree_and_undo_removes_it() {
        let d = TestDir::new("transferpartialtree");
        let src = d.dir("tree");
        std::fs::write(src.join("good.txt"), "body").unwrap();
        let shut = src.join("shut.txt");
        std::fs::write(&shut, "body").unwrap();
        std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
        let dest = d.dir("out");
        let (tx, rx) = channel();
        run_transfer(5, false, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
        let (ok, failed, _, cancelled, entry) = done_line(rx);
        assert_eq!((ok, failed, cancelled), (0, 1, false));
        let partial = dest.join("tree");
        assert!(partial.is_dir(), "a failure that is not a cancel leaves what it copied");
        assert_eq!(entry.steps, vec![Step::Created { path: partial.clone() }], "the partial tree is journaled");
        let mut j = Journal::new();
        j.push(entry);
        assert_eq!(j.undo().expect("undo"), "copy");
        assert!(!partial.exists(), "undo removed the partial tree");
        assert!(src.join("good.txt").exists(), "and left the source alone");
    }

    #[test]
    fn a_copy_refused_at_an_existing_destination_records_nothing_to_undo() {
        let d = TestDir::new("transferclobber");
        let src = d.file("a.txt", "new");
        let dest = d.dir("out");
        std::fs::write(dest.join("a.txt"), "already here").unwrap();
        let (tx, rx) = channel();
        run_transfer(6, false, vec![src.to_string_lossy().to_string()], dest.clone(), Arc::new(AtomicBool::new(false)), tx);
        let (_, failed, _, _, entry) = done_line(rx);
        assert_eq!(failed, 1);
        assert!(entry.steps.is_empty(), "undo must never remove what the user already had");
        assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "already here");
    }

    #[test]
    fn a_cancelled_transfer_skips_the_rest_and_says_so() {
        let d = TestDir::new("transfercancel");
        let a = d.file("a.txt", "one");
        let b = d.file("b.txt", "two");
        let dest = d.dir("out");
        let (tx, rx) = channel();
        run_transfer(
            4,
            false,
            vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()],
            dest.clone(),
            Arc::new(AtomicBool::new(true)),
            tx,
        );
        let (ok, _, skipped, cancelled, _) = done_line(rx);
        assert_eq!((ok, skipped, cancelled), (0, 2, true));
        assert!(!dest.join("a.txt").exists(), "a cancel before the first item copies nothing");
    }
}
