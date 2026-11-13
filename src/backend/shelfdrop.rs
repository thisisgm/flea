// A drop out of the shelf, redeemed. DragOut rule 1: the token names the entries and the intent, and
// the transfer engine the rest of the app uses runs it; rule 3: the pile changes only on completion.
use crate::backend::collide::{Ask, Policy};
use crate::backend::opsdispatch::Ops;
use crate::backend::opsreq::{op_err, run_transfer_checked, transferstarted_line, usable_dest, OpMsg};
use crate::backend::proto::error_line;
use crate::backend::undo::Entry;
use crate::shelf::{now_ms, Shelf};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

// Rule 4: a token that is unknown, expired, already spent or no longer names the files it was minted
// for is refused with a sentence. Flea never falls back to a URI copy after promising a move.
pub(crate) fn start(out: &mut impl Write, ops: &mut Ops, token: &str, dest: &str, collide: Ask) {
    start_from(out, ops, Shelf::user, token, dest, collide)
}

// The shelf is opened here, after the live and landing checks, so a test can hand in a sandboxed one.
fn start_from(
    out: &mut impl Write,
    ops: &mut Ops,
    open: impl FnOnce() -> Result<Shelf, String>,
    token: &str,
    dest: &str,
    collide: Ask,
) {
    // Redeeming spends the token whatever happens next, so every refusal this drop can earn is
    // earned before it: a burned token leaves the operator with a drag they cannot repeat.
    if ops.live.running().is_some() {
        return refuse(out, dest, "an operation is already running");
    }
    let landing = match usable_dest(dest) {
        Ok(landing) => landing,
        Err(e) => {
            writeln!(out, "{}", error_line(&e)).ok();
            out.flush().ok();
            return;
        }
    };
    let shelf = match open() {
        Ok(shelf) => shelf,
        Err(e) => return refuse(out, dest, &e),
    };
    let redeemed = match shelf.redeem(token, now_ms()) {
        Ok(redeemed) => redeemed,
        Err(e) => return refuse(out, dest, &e),
    };
    let policy = collide.policy(ops.question.take(), &landing);
    let dest = landing;
    let (id, cancel) = ops.claim_transfer();
    writeln!(out, "{}", transferstarted_line(id, redeemed.paths.len(), redeemed.moving)).ok();
    // The client sent only the token, so the shelf's own entries are named for its eject check.
    let mut roots = redeemed.paths.clone();
    roots.push(dest.to_string_lossy().into_owned());
    let kind = if redeemed.moving { "move" } else { "copy" };
    writeln!(
        out,
        "{}",
        crate::backend::jobview::roots_line(id, false, kind, None, &roots)
    )
    .ok();
    out.flush().ok();
    let tx = ops.tx.clone();
    thread::spawn(move || run_and_settle(id, redeemed, dest, cancel, tx, shelf, policy));
}

// The pile is updated from what the engine reported, not from what the drag asked for: an item that
// failed or was skipped stays on the shelf, and a copy leaves every reference where it was.
fn run_and_settle(
    id: usize,
    redeemed: crate::shelfdrag::Redeemed,
    dest: PathBuf,
    cancel: Arc<AtomicBool>,
    tx: Sender<OpMsg>,
    shelf: Shelf,
    policy: Policy,
) {
    let crate::shelfdrag::Redeemed { moving, paths } = redeemed;
    // The engine's own channel, watched on the way past: the client still sees every line unchanged.
    let (mine, watch) = channel::<OpMsg>();
    let watched = paths.clone();
    let done_tx = tx.clone();
    let total = paths.len();
    // Held here rather than inside the thread: the client's one terminal message has to be sent
    // whether the bookkeeping finished or panicked, or the pane waits on a transfer forever.
    let held: Arc<Mutex<Option<OpMsg>>> = Arc::new(Mutex::new(None));
    let carried = Arc::clone(&held);
    let forward = thread::spawn(move || {
        let mut reported = Vec::new();
        for msg in watch {
            if let OpMsg::Item { index, ok, .. } = &msg {
                reported.push((*index, *ok));
            }
            // Withheld until the pile has been settled: a client told the transfer finished must
            // never read a pile that still lists what the move took away.
            if matches!(msg, OpMsg::TransferDone { .. }) {
                if let Ok(mut slot) = carried.lock() {
                    *slot = Some(msg);
                }
                continue;
            }
            let _ = tx.send(msg);
        }
        if let Err(e) = shelf.settle(&moved_paths(&reported, &watched, moving)) {
            eprintln!("flea: the shelf kept its references ({})", e);
        }
    });
    // The guard owns the join, because a JoinHandle dropped any other way only detaches its thread.
    let _terminal = Terminal { tx: done_tx, held: Arc::clone(&held), forward: Some(forward), id, total, moving };
    run_transfer_checked(id, moving, paths, dest, cancel, mine, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: policy });
}

// A guard, because the engine runs on this thread and a panic there unwinds past any send after it.
struct Terminal {
    tx: Sender<OpMsg>,
    held: Arc<Mutex<Option<OpMsg>>>,
    forward: Option<thread::JoinHandle<()>>,
    id: usize,
    total: usize,
    moving: bool,
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // The engine's end of the channel has gone either way, so this waits only for the settle.
        if let Some(forward) = self.forward.take() {
            if forward.join().is_err() {
                eprintln!("flea: the shelf's own bookkeeping stopped before it finished");
            }
        }
        let sent = match self.held.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        // The engine's own message, or a failure for everything the drop was given when it sent none.
        let _ = self.tx.send(sent.unwrap_or_else(|| OpMsg::TransferDone {
            id: self.id,
            ok: 0,
            failed: self.total,
            skipped: 0,
            cancelled: false,
            entry: Entry {
                op: if self.moving { "move".to_string() } else { "copy".to_string() },
                steps: Vec::new(),
            },
            retry: Vec::new(),
            durable: false,
            note: String::new(),
            refused: Vec::new(),
        }));
    }
}

// What leaves the pile: the entries the engine reported moved, so a copy settles nothing and an
// entry that failed or was skipped stays on the shelf.
fn moved_paths(reported: &[(usize, bool)], paths: &[String], moving: bool) -> Vec<String> {
    if !moving {
        return Vec::new();
    }
    let mut moved = Vec::new();
    for (index, ok) in reported {
        if *ok {
            if let Some(path) = paths.get(*index) {
                moved.push(path.clone());
            }
        }
    }
    moved
}

fn refuse(out: &mut impl Write, dest: &str, why: &str) {
    writeln!(out, "{}", error_line(&op_err("transfer", dest, why))).ok();
    out.flush().ok();
}

#[cfg(test)]
mod tests {
    use super::{moved_paths, start, start_from, Ask, OpMsg, Ops, Shelf};
    use crate::backend::testdir::TestDir;

    #[test]
    fn a_drop_while_an_operation_runs_says_the_shared_sentence() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut ops = Ops::new(tx);
        let _claim = ops.claim_transfer();
        let mut out = Vec::new();
        start(&mut out, &mut ops, "token", "/d", Ask::parse(r#"{"c":"transfer"}"#));
        let line = String::from_utf8(out).unwrap();
        assert!(line.contains(r#""msg":"an operation is already running""#), "{}", line);
    }

    #[test]
    fn a_drop_names_the_shelf_entries_it_reads_and_the_folder_it_writes() {
        for (moving, kind) in [(false, "copy"), (true, "move")] {
            let root = TestDir::new(&format!("shelf-drop-roots-{kind}"));
            root.dir("stick");
            let photo = root
                .file("stick/photo.jpg", "jpeg")
                .to_string_lossy()
                .into_owned();
            let dest = root.dir("home");
            let shelf = Shelf::at(&root.join("state"));
            let token = shelf
                .drag_begin(moving, std::slice::from_ref(&photo), crate::shelf::now_ms())
                .unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            let mut ops = Ops::new(tx);
            let mut out = Vec::new();
            start_from(
                &mut out,
                &mut ops,
                move || Ok(shelf),
                &token,
                dest.to_str().unwrap(),
                Ask::parse(r#"{"c":"transfer"}"#),
            );
            let text = String::from_utf8(out).unwrap();
            let roots = text
                .lines()
                .find(|line| line.starts_with(r#"{"t":"roots""#))
                .unwrap_or_default();
            assert!(roots.contains(r#""job":false"#), "a drop is no job: {text}");
            assert!(
                roots.contains(&format!(r#""kind":"{kind}""#)),
                "a drop names its kind: {text}"
            );
            assert_eq!(
                crate::json::field_str_array(roots, "paths"),
                [photo, dest.to_string_lossy().into_owned()],
                "{text}"
            );
            while !matches!(
                rx.recv_timeout(std::time::Duration::from_secs(10))
                    .expect("the drop ends"),
                OpMsg::TransferDone { .. }
            ) {}
        }
    }

    #[test]
    fn only_what_the_engine_moved_leaves_the_pile() {
        let paths = vec!["/p/one".to_string(), "/p/two".to_string(), "/p/three".to_string()];
        let reported = vec![(0, true), (1, false), (2, true)];
        assert_eq!(moved_paths(&reported, &paths, true), vec!["/p/one", "/p/three"],
                   "the one that failed stays on the shelf");
        assert!(moved_paths(&reported, &paths, false).is_empty(),
                "a copy leaves every reference where it was");
        assert!(moved_paths(&[], &paths, true).is_empty(),
                "an engine that reported nothing settles nothing");
        assert!(moved_paths(&[(9, true)], &paths, true).is_empty(),
                "and an index past the end names no entry");
    }
}
