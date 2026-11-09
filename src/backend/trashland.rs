// A trash batch's worker and its landing: the line and the slot go at the announce, the journal entry after the settle list.
use crate::backend::opsdispatch::Ops;
use crate::backend::opsreq::{op_err, trashed_line, OpMsg};
use crate::backend::trash;
use crate::backend::undo::{Entry, Step};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::Sender;

pub(crate) fn run_trash(id: usize, request: usize, paths: Vec<String>, tx: Sender<OpMsg>, selection: Option<Vec<super::menu_actions::Selected>>) {
    let owned: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    // Undo and redo name this path while the batch's entry is pending; a leading empty path is a drag marker, not an item.
    let first = paths.iter().find(|p| !p.is_empty()).cloned().unwrap_or_default();
    let mut announced = false;
    let early = tx.clone();
    let mut announce = |moved: &[PathBuf], failed: usize, reason: &str| {
        let reason = read_only_reason(&owned, moved, failed, reason);
        let moved: Vec<String> = moved.iter().map(|path| path.to_string_lossy().into_owned()).collect();
        let ok = moved.len();
        announced = early.send(OpMsg::TrashAnnounced { id, request, path: first.clone(), ok, failed, reason, moved }).is_ok();
    };
    let (entries, failed, reason) = match trash::trash_announcing(&owned, selection.as_deref(), &mut announce) {
        Ok(result) => result,
        Err(error) => {
            let line = super::proto::error_line(&op_err("trash", "", &error));
            let _ = tx.send(OpMsg::Meta { line });
            (Vec::new(), owned.len(), error)
        }
    };
    let ok = entries.len();
    let gone: Vec<PathBuf> = entries.iter().map(|entry| entry.original.clone()).collect();
    let reason = read_only_reason(&owned, &gone, failed, &reason);
    let steps = entries.into_iter().map(Step::Trashed).collect();
    let entry = Entry { op: "trash".to_string(), steps };
    let _ = tx.send(OpMsg::Trashed { id, request, ok, failed, entry, reason, announced });
}

// linux/errno.h's read-only filesystem, whose sentence src/error.rs already holds.
const EROFS: i32 = 30;

// A read-only drive has no Trash folder gio can make, so items left there say the drive is read-only, as every other write does.
fn read_only_reason(paths: &[PathBuf], moved: &[PathBuf], failed: usize, reason: &str) -> String {
    let body = super::iomount::mount_body();
    let mut left = paths.iter().filter(|path| !path.as_os_str().is_empty() && !moved.contains(path) && path.symlink_metadata().is_ok()).peekable();
    if failed > 0 && left.peek().is_some() && left.all(|path| read_only_in(path, &body)) {
        return crate::error::from_io("trash", "", &std::io::Error::from_raw_os_error(EROFS)).msg;
    }
    reason.to_string()
}

// Sample input: "36 1 7:3 / /run/media/gm/S8RO ro,nosuid - ext4 /dev/loop3 ro" answers true for a path under that mount.
fn read_only_in(path: &std::path::Path, body: &str) -> bool {
    let ro = |options: Option<String>| options.is_some_and(|options| options.split(',').any(|option| option == "ro"));
    ro(super::mountinfo::raw_mount_options_in(path, body)) || ro(super::mountinfo::mount_options_in(path, body))
}

// The first batch whose entry is still pending, which is the path undo and redo answer busy with.
pub(crate) fn settling_path(ops: &Ops) -> Option<String> {
    ops.settling.first().map(|(_, path)| path.clone())
}

// The announce writes the line and frees the slot, so any write runs and only undo and redo wait; the journal message pushes the entry and ends that wait.
pub(crate) fn land(out: &mut impl Write, ops: &mut Ops, msg: OpMsg) {
    match msg {
        OpMsg::TrashAnnounced { id, request, path, ok, failed, reason, moved } => {
            ops.settling.push((id, path));
            ops.journal.reserve(id);
            ops.live.finished_if(id);
            writeln!(out, "{}", trashed_line(request, ok, failed, &reason, &moved)).ok();
        }
        OpMsg::Trashed { id, request, ok, failed, entry, reason, announced } => {
            let moved: Vec<String> = entry.steps.iter().filter_map(|step| match step {
                Step::Trashed(gone) => Some(gone.original.to_string_lossy().into_owned()),
                _ => None,
            }).collect();
            super::renamework::publish(ops, super::renamework::Publication::Fill(id, entry));
            ops.settling.retain(|(held, _)| *held != id);
            ops.live.finished_if(id);
            if !announced { writeln!(out, "{}", trashed_line(request, ok, failed, &reason, &moved)).ok(); }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::dirsizeworker::Worker;
    use crate::backend::events::Event;
    use crate::backend::flashslot;
    use crate::backend::opsdispatch::{do_undo, start_trash};
    use crate::backend::run::drain;
    use crate::backend::state::{State, Tables};
    use crate::backend::testdir::TestDir;
    use crate::backend::thumbcache::Cache;
    use crate::backend::thumbs::Pool;

    fn land(out: &mut Vec<u8>, ops: &mut Ops, message: OpMsg) {
        super::super::renamework::test_request(out, ops, |out, ops| { super::land(out, ops, message); Vec::new() });
    }

    fn do_rename(out: &mut Vec<u8>, ops: &mut Ops, path: &str, to: &str) {
        super::super::renamework::test_request(out, ops, |out, ops| super::super::opsdispatch::do_rename(out, ops, path, to));
    }

    fn created(path: &std::path::Path) -> Entry {
        Entry { op: "trash".to_string(), steps: vec![Step::Created { path: path.to_path_buf() }] }
    }

    fn announced(id: usize, path: &std::path::Path) -> OpMsg {
        OpMsg::TrashAnnounced { id, request: 0, path: path.to_string_lossy().into(), ok: 1, failed: 0, reason: String::new(), moved: vec![path.to_string_lossy().into()] }
    }

    fn journaled(id: usize, path: &std::path::Path) -> OpMsg {
        OpMsg::Trashed { id, request: 0, ok: 1, failed: 0, entry: created(path), reason: String::new(), announced: true }
    }

    #[test]
    fn an_announced_trash_writes_its_line_once_and_frees_the_slot_at_the_announce() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut o = Ops::new(tx);
        let (id, _) = o.claim_transfer();
        let mut buf: Vec<u8> = Vec::new();
        land(&mut buf, &mut o, announced(id, std::path::Path::new("/x")));
        assert_eq!(String::from_utf8_lossy(&buf).trim(), r#"{"t":"trashed","id":0,"ok":1,"failed":0,"moved":["/x"]}"#);
        assert!(o.live.running().is_none(), "the announce frees the slot, so any write runs while the settle list is out");
        assert_eq!(o.journal.len(), 0, "the entry stays pending until the settle list lands");
        land(&mut buf, &mut o, journaled(id, std::path::Path::new("/x")));
        assert_eq!(String::from_utf8_lossy(&buf).matches(r#""t":"trashed""#).count(), 1, "the wire carries one line per batch");
        assert_eq!(o.journal.len(), 1);
        assert!(o.settling.is_empty(), "the journal ends the wait");
    }

    // The line names the request it answers and the paths that moved, so the client records only those.
    #[test]
    fn a_trash_line_carries_its_request_and_the_paths_that_moved() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut o = Ops::new(tx);
        let (id, _) = o.claim_transfer();
        let mut buf: Vec<u8> = Vec::new();
        land(&mut buf, &mut o, announced(id, std::path::Path::new("/x")));
        assert_eq!(String::from_utf8_lossy(&buf).trim(), r#"{"t":"trashed","id":0,"ok":1,"failed":0,"moved":["/x"]}"#);
    }

    #[test]
    fn an_undo_between_the_announce_and_the_settle_list_waits_then_reverses_this_trash() {
        let d = TestDir::new("trashland-undo");
        let (older, trashed) = (d.file("older.txt", "o"), d.file("trashed.txt", "t"));
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut o = Ops::new(tx);
        o.journal.push(created(&older));
        let (id, _) = o.claim_transfer();
        let mut buf: Vec<u8> = Vec::new();
        land(&mut buf, &mut o, announced(id, &trashed));
        let mut blocked: Vec<u8> = Vec::new();
        do_undo(&mut blocked, &mut o);
        let line = String::from_utf8_lossy(&blocked).to_string();
        assert!(line.contains("already running") && line.contains(&*trashed.to_string_lossy()), "undo waits on this batch's entry and names it: {line}");
        assert!(older.exists() && o.journal.len() == 1, "the earlier entry is never reversed in the window");
        land(&mut buf, &mut o, journaled(id, &trashed));
        let mut undone: Vec<u8> = Vec::new();
        do_undo(&mut undone, &mut o);
        assert!(String::from_utf8_lossy(&undone).contains(r#""t":"undone""#), "{}", String::from_utf8_lossy(&undone));
        assert!(!trashed.exists() && older.exists(), "the first undo after the settle list reverses this trash, not the earlier entry");
    }

    #[test]
    fn a_trash_sent_in_the_window_is_not_refused_and_the_old_landing_keeps_its_slot() {
        let d = TestDir::new("trashland-next");
        let (first, missing) = (d.file("first.txt", "f"), d.join("missing.txt"));
        let (tx, rx) = std::sync::mpsc::channel();
        let mut o = Ops::new(tx);
        let (id, _) = o.claim_transfer();
        let mut buf: Vec<u8> = Vec::new();
        land(&mut buf, &mut o, announced(id, &first));
        let mut next: Vec<u8> = Vec::new();
        start_trash(&mut next, &mut o, vec![missing.to_string_lossy().into()], 0, 0);
        let second = o.live.running().expect("the second trash claimed the slot");
        assert_eq!(String::from_utf8_lossy(&next), format!(r#"{{"t":"trashstarted","id":{second},"n":1,"cancellable":false}}"#) + "\n", "a second trash acknowledges acceptance without any refusal or extra line");
        land(&mut buf, &mut o, journaled(id, &first));
        assert_eq!(o.live.running(), Some(second), "the first batch landing never frees the second batch's slot");
        let done = rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the second trash reports");
        assert!(matches!(done, OpMsg::Trashed { announced: false, .. }));
    }

    fn emptied(id: usize) -> OpMsg {
        OpMsg::Trashed { id, request: 0, ok: 0, failed: 1, entry: Entry { op: "trash".to_string(), steps: Vec::new() }, reason: String::new(), announced: true }
    }

    fn undone(o: &mut Ops) -> String {
        let mut line: Vec<u8> = Vec::new();
        do_undo(&mut line, o);
        String::from_utf8_lossy(&line).to_string()
    }

    #[test]
    fn a_rename_in_the_settle_window_journals_after_the_trash_so_undo_reverses_the_rename_first() {
        let d = TestDir::new("trashland-order-rename");
        let (x, trashed) = (d.file("x.txt", "x"), d.file("trashed.txt", "t"));
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut o = Ops::new(tx);
        let (id, _) = o.claim_transfer();
        let mut buf: Vec<u8> = Vec::new();
        land(&mut buf, &mut o, announced(id, &trashed));
        do_rename(&mut buf, &mut o, &x.to_string_lossy(), "y.txt");
        assert!(d.join("y.txt").exists() && o.journal.is_empty(), "the rename ran and waits behind the batch's place");
        land(&mut buf, &mut o, journaled(id, &trashed));
        assert_eq!(o.journal.len(), 2, "the trash and the rename both filed once the settle list landed");
        assert!(undone(&mut o).contains(r#""t":"undone""#));
        assert!(x.exists() && trashed.exists(), "the first undo reverses the rename, the user's last action, and leaves the trash");
        assert!(undone(&mut o).contains(r#""t":"undone""#));
        assert!(!trashed.exists(), "the second undo reverses the trash");
    }

    #[test]
    fn two_batches_whose_settle_lists_finish_in_reverse_journal_in_the_order_they_were_announced() {
        let d = TestDir::new("trashland-order-batches");
        let (a, b) = (d.file("a.txt", "a"), d.file("b.txt", "b"));
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut o = Ops::new(tx);
        let mut buf: Vec<u8> = Vec::new();
        let (first, _) = o.claim_transfer();
        land(&mut buf, &mut o, announced(first, &a));
        let (second, _) = o.claim_transfer();
        land(&mut buf, &mut o, announced(second, &b));
        land(&mut buf, &mut o, journaled(second, &b));
        assert!(o.journal.is_empty(), "the later batch waits for the earlier one's place");
        assert!(undone(&mut o).contains("already running") && b.exists());
        land(&mut buf, &mut o, journaled(first, &a));
        assert_eq!(o.journal.len(), 2);
        assert!(undone(&mut o).contains(r#""t":"undone""#));
        assert!(a.exists() && !b.exists(), "the first undo reverses the later batch");
        assert!(undone(&mut o).contains(r#""t":"undone""#));
        assert!(!a.exists(), "the second undo reverses the earlier batch");
    }

    #[test]
    fn a_batch_that_landed_nothing_still_closes_its_place_and_the_queued_rename_journals() {
        let d = TestDir::new("trashland-order-empty");
        let (x, missing) = (d.file("x.txt", "x"), d.join("missing.txt"));
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut o = Ops::new(tx);
        let (id, _) = o.claim_transfer();
        let mut buf: Vec<u8> = Vec::new();
        land(&mut buf, &mut o, announced(id, &missing));
        do_rename(&mut buf, &mut o, &x.to_string_lossy(), "y.txt");
        assert!(o.journal.is_empty());
        land(&mut buf, &mut o, emptied(id));
        assert_eq!(o.journal.len(), 1, "the rename journals and the empty entry adds nothing");
        assert!(undone(&mut o).contains(r#""t":"undone""#), "undo is free again");
        assert!(x.exists() && !d.join("y.txt").exists());
    }

    // A quit's drain over the parts it reads; the worker keeps its own channel, so dropping the sender closes the loop's.
    fn quit_rig(d: &TestDir) -> (State, Pool, Cache, crate::backend::events::Sender<Event>, std::sync::mpsc::Receiver<Event>, flashslot::Ctx) {
        let tb = Tables::load();
        let (done_tx, _done_rx) = std::sync::mpsc::channel();
        let pool = Pool::new(1, done_tx, d.join("thumbs"), std::sync::Arc::clone(&tb.aliases), std::sync::Arc::clone(&tb.thumbs));
        let (worker_tx, _worker_rx) = crate::backend::events::channel();
        let (ev_tx, ev_rx) = crate::backend::events::channel();
        let (flash_tx, _flash_rx) = std::sync::mpsc::channel();
        (State::new(Worker::new(worker_tx)), pool, Cache::at(d.join("cache")), ev_tx, ev_rx, flashslot::Ctx::new(flash_tx))
    }

    #[test]
    fn a_quit_in_the_settle_window_waits_for_the_trash_entry_and_files_the_rename_behind_it() {
        let d = TestDir::new("trashland-quit-wait");
        let (x, trashed) = (d.file("x.txt", "x"), d.file("trashed.txt", "t"));
        let (mut st, pool, cache, ev_tx, ev_rx, mut flash) = quit_rig(&d);
        let (tx, op_rx) = std::sync::mpsc::channel();
        let events = ev_tx.clone();
        std::thread::spawn(move || { for message in op_rx { if events.send(Event::Op(message)).is_err() { break; } } });
        let mut o = Ops::new(tx);
        let (id, _) = o.claim_transfer();
        let mut buf: Vec<u8> = Vec::new();
        land(&mut buf, &mut o, announced(id, &trashed));
        do_rename(&mut buf, &mut o, &x.to_string_lossy(), "y.txt");
        let late = journaled(id, &trashed);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = ev_tx.send(Event::Op(late));
        });
        drain(&mut buf, &mut st, &mut o, &mut flash, &ev_rx, &pool, &cache);
        assert_eq!(o.journal.len(), 2, "the drain left before the settle list landed, so the trash entry died with the window");
        assert!(undone(&mut o).contains(r#""t":"undone""#));
        assert!(x.exists() && trashed.exists(), "the rename is still the last action after the quit");
    }

    #[test]
    fn a_quit_cut_short_before_the_settle_list_lands_still_files_the_rename_behind_it() {
        let d = TestDir::new("trashland-quit-cut");
        let (x, trashed) = (d.file("x.txt", "x"), d.file("trashed.txt", "t"));
        let (mut st, pool, cache, ev_tx, ev_rx, mut flash) = quit_rig(&d);
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut o = Ops::new(tx);
        let (id, _) = o.claim_transfer();
        let mut buf: Vec<u8> = Vec::new();
        land(&mut buf, &mut o, announced(id, &trashed));
        do_rename(&mut buf, &mut o, &x.to_string_lossy(), "y.txt");
        drop(ev_tx);
        drain(&mut buf, &mut st, &mut o, &mut flash, &ev_rx, &pool, &cache);
        assert_eq!(o.journal.len(), 1, "the rename held behind the open place died with the window");
        // The window that undoes it next owes no settle list, as a fresh backend over the shared journal would not.
        o.settling.clear();
        assert!(undone(&mut o).contains(r#""t":"undone""#));
        assert!(x.exists() && !d.join("y.txt").exists(), "the filed entry is the rename");
    }

    #[test]
    fn items_left_on_a_read_only_drive_say_so_instead_of_gios_words() {
        let d = TestDir::new("trashland-read-only");
        let (locked, other) = (d.file("locked.bin", "x"), d.file("other.bin", "y"));
        let gio = "gio: file:///x: Unable to find or create trash directory";
        let mount = |options: &str| format!("36 1 7:3 / {} {options} - ext4 /dev/loop3 {options}\n", d.path().display());
        let paths = vec![locked.clone(), other.clone()];
        let _held = crate::backend::iomount::test_hold_body(mount("ro,nosuid"));
        assert_eq!(read_only_reason(&paths, &[], 2, gio), "the drive is read-only");
        assert_eq!(read_only_reason(&paths, std::slice::from_ref(&other), 1, gio), "the drive is read-only", "only the item left behind is judged");
        assert_eq!(read_only_reason(&paths, &paths, 0, ""), "", "nothing failed, so nothing is said");
        drop(_held);
        let _held = crate::backend::iomount::test_hold_body(mount("rw,nosuid"));
        assert_eq!(read_only_reason(&paths, &[], 2, gio), gio, "a writable drive keeps gio's own reason");
    }
}
