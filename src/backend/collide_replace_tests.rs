// Replace, its undo and its redo, against a stand-in trash inside the sandbox.
use super::tests::{asked, chosen, owned, refusing_trash, text, transfer, StandIn};
use super::*;
use crate::backend::mergeapply::{replacing_deferred, Deferred};
use crate::backend::opsreq::run_transfer_checked;
use crate::backend::testdir::TestDir;
use crate::backend::trash;
use crate::backend::undo::Journal;
use crate::error::FleaError;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;

// Numbers the stand-in's entries, so two items trashed from one path never share a URI.
static NEXT_ENTRY: AtomicUsize = AtomicUsize::new(0);
const URI: &str = "trash:///";
const ORIGINAL: &str = ".original";

// A wait status carries the exit code in its second byte.
const EXIT_CODE_SHIFT: i32 = 8;

fn exited(code: i32, stdout: String) -> Output {
    Output { status: ExitStatus::from_raw(code << EXIT_CODE_SHIFT), stdout: stdout.into_bytes(), stderr: Vec::new() }
}

// gio's trash, played by a folder inside the sandbox: entry n sits at can/n with its original path beside it.
fn gio_trash(can: &Path, args: &[&str]) -> Output {
    match args {
        ["trash", "--list"] => {
            let mut listed = String::new();
            for item in std::fs::read_dir(can).unwrap().flatten() {
                let name = item.file_name().to_string_lossy().to_string();
                if let Some(n) = name.strip_suffix(ORIGINAL) {
                    listed.push_str(&format!("{}{}\t{}\n", URI, n, std::fs::read_to_string(item.path()).unwrap()));
                }
            }
            exited(0, listed)
        }
        ["trash", "--restore", uri] => {
            let n = uri.strip_prefix(URI).unwrap();
            let original = PathBuf::from(std::fs::read_to_string(can.join(format!("{}{}", n, ORIGINAL))).unwrap());
            if original.symlink_metadata().is_ok() {
                return exited(1, String::new());
            }
            std::fs::rename(can.join(n), &original).unwrap();
            std::fs::remove_file(can.join(format!("{}{}", n, ORIGINAL))).unwrap();
            exited(0, String::new())
        }
        ["trash", "--", paths @ ..] => {
            for path in paths {
                let n = NEXT_ENTRY.fetch_add(1, Ordering::Relaxed).to_string();
                std::fs::rename(path, can.join(&n)).unwrap();
                std::fs::write(can.join(format!("{}{}", n, ORIGINAL)), path).unwrap();
            }
            exited(0, String::new())
        }
        _ => exited(1, String::new()),
    }
}

pub(super) fn working_trash(d: &TestDir) -> (StandIn, PathBuf) {
    let can = d.dir("can");
    let at = can.clone();
    trash::STAND_IN.with(|slot| *slot.borrow_mut() = Some(Rc::new(move |args: &[&str]| Some(gio_trash(&at, args)))));
    (StandIn, can)
}

// The working stand-in whose restore always fails, the way gio answers once the entry has left the trash; each trash presses Cancel, and lifts says whether each restore lifts it again.
fn unrestorable_trash(d: &TestDir, cancel: Arc<AtomicBool>, lifts: bool) -> StandIn {
    let can = d.dir("can");
    trash::STAND_IN.with(|slot| *slot.borrow_mut() = Some(Rc::new(move |args: &[&str]| Some(match args {
        ["trash", "--restore", ..] => {
            if lifts {
                cancel.store(false, Ordering::Relaxed);
            }
            exited(1, String::new())
        }
        ["trash", "--", ..] => {
            cancel.store(true, Ordering::Relaxed);
            gio_trash(&can, args)
        }
        _ => gio_trash(&can, args),
    }))));
    StandIn
}

fn redo(journal: &mut Journal) -> Result<String, FleaError> {
    let (tx, _rx) = channel();
    journal.redo(1, &AtomicBool::new(false), &tx)
}

#[test]
fn replace_trashes_the_old_item_first_and_undo_and_redo_swap_them_back() {
    let d = TestDir::new("collide-replace");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let (_trash, can) = working_trash(&d);
    let (ok, failed, _, _, entry) = transfer(false, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(text(&there), "yours");
    assert!(matches!(&entry.steps[..], [Step::Trashed(old), Step::Copied { .. }] if old.original == there), "{:?}", entry.steps);
    assert_eq!(std::fs::read_dir(&can).unwrap().count(), 2, "the old item and its original path are in the trash, not deleted");
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&there);
    assert_eq!(journal.undo().unwrap(), "copy");
    assert_eq!(text(&there), "there", "one undo takes the new item away and restores the old one to its place");
    assert_eq!(text(&photo), "yours");
    assert_eq!(redo(&mut journal).unwrap(), "copy");
    assert_eq!(text(&there), "yours", "redo replaces again, through the trash again");
    journal.undo().unwrap();
    assert_eq!(text(&there), "there");
}

#[test]
fn a_moved_replace_undoes_to_both_original_places() {
    let d = TestDir::new("collide-replace-move");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let (_trash, _can) = working_trash(&d);
    let (ok, _, _, _, entry) = transfer(true, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    assert_eq!(ok, 1);
    assert!(!photo.exists());
    assert_eq!(text(&there), "yours");
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&there);
    assert_eq!(journal.undo().unwrap(), "move");
    assert_eq!((text(&photo), text(&there)), ("yours".to_string(), "there".to_string()));
}

#[test]
fn a_colliding_folder_is_replaced_whole_and_never_merged() {
    let d = TestDir::new("collide-folder");
    let to = d.dir("to");
    let album = d.dir("from/album");
    d.file("from/album/new.txt", "new");
    d.dir("to/album");
    d.file("to/album/old.txt", "old");
    let (_trash, _can) = working_trash(&d);
    let (ok, _, _, _, entry) = transfer(false, &[&album], &to, chosen("replace", asked(1, &[&album], &to), &to));
    assert_eq!(ok, 1);
    assert!(to.join("album/new.txt").exists() && !to.join("album/old.txt").exists(), "the old folder went whole, nothing was merged");
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&to.join("album"));
    journal.undo().unwrap();
    assert!(to.join("album/old.txt").exists() && !to.join("album/new.txt").exists());
}

#[test]
fn when_trash_refuses_nothing_is_replaced_and_nothing_is_journaled() {
    let d = TestDir::new("collide-refused");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let _trash = refusing_trash();
    let (ok, failed, _, errors, entry) = transfer(false, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    assert_eq!((ok, failed), (0, 1));
    assert_eq!(errors, vec![TRASH_REFUSED.to_string()]);
    assert_eq!((text(&there), text(&photo)), ("there".to_string(), "yours".to_string()));
    assert!(entry.steps.is_empty());
}

#[test]
fn a_replace_whose_copy_fails_puts_the_old_item_straight_back() {
    let d = TestDir::new("collide-putback");
    let to = d.dir("to");
    d.dir("from");
    let shut = d.file("from/shut.txt", "unreadable");
    let there = d.file("to/shut.txt", "there");
    let (_trash, can) = working_trash(&d);
    let question = asked(1, &[&shut], &to);
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (ok, failed, _, errors, entry) = transfer(false, &[&shut], &to, chosen("replace", question, &to));
    std::fs::set_permissions(&shut, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!((ok, failed), (0, 1));
    assert_eq!(errors, vec!["permission denied".to_string()]);
    assert_eq!(text(&there), "there", "the copy never started, so the old item is back in its place");
    assert!(entry.steps.is_empty(), "and there is nothing left for undo to do");
    assert_eq!(std::fs::read_dir(&can).unwrap().count(), 0);
    // A cancel reaches the same path and keeps its own word, which is what the transfer counts it by.
    let cancelled = |_: &mut Vec<Step>| Err(FleaError { where_: "copy".into(), path: String::new(), msg: CANCELLED.into() });
    let mut deferred = Deferred::begin();
    let mut steps = Vec::new();
    assert_eq!(replacing_deferred(&there, &mut steps, &mut deferred, &std::sync::atomic::AtomicBool::new(false), cancelled).unwrap_err().msg, CANCELLED);
    assert_eq!(text(&there), "there");
    assert!(steps.is_empty());
}

#[test]
fn replace_refuses_an_item_that_holds_the_one_being_moved_in() {
    let d = TestDir::new("collide-holds");
    let outer = d.dir("x");
    let inner = d.dir("x/x");
    let _trash = refusing_trash();
    let (ok, failed, _, errors, _) = transfer(true, &[&inner], d.path(), chosen("replace", asked(1, &[&inner], d.path()), d.path()));
    assert_eq!((ok, failed, errors), (0, 1, vec![HOLDS_SOURCE.to_string()]));
    assert!(outer.is_dir() && inner.is_dir());
}

#[test]
fn a_put_back_that_fails_keeps_the_step_for_undo_and_says_the_old_item_is_in_trash() {
    let d = TestDir::new("collide-putback-fails");
    d.dir("to");
    let there = d.file("to/shut.txt", "there");
    let _trash = unrestorable_trash(&d, Arc::new(AtomicBool::new(false)), true);
    let failing = |_: &mut Vec<Step>| Err(FleaError { where_: "copy".into(), path: String::new(), msg: "permission denied".into() });
    let mut deferred = Deferred::begin();
    let mut steps = Vec::new();
    let error = replacing_deferred(&there, &mut steps, &mut deferred, &std::sync::atomic::AtomicBool::new(false), failing).unwrap_err();
    assert!(error.msg.starts_with("permission denied; the item it replaced is still in the Trash"), "{}", error.msg);
    assert!(matches!(&steps[..], [Step::Trashed(old)] if old.original == there), "undo can still restore it: {:?}", steps);
}

#[test]
fn a_cancelled_replace_whose_put_back_fails_is_a_failure_and_its_own_error_stops_the_batch() {
    let d = TestDir::new("collide-cancel-putback");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let later = d.file("from/later.txt", "never reached");
    let there = d.file("to/photo.png", "there");
    let cancel = Arc::new(AtomicBool::new(false));
    let _trash = unrestorable_trash(&d, Arc::clone(&cancel), true);
    let (tx, rx) = channel();
    let policy = chosen("replace", asked(1, &[&photo, &later], &to), &to);
    run_transfer_checked(1, false, owned(&[&photo, &later]), to.clone(), Arc::clone(&cancel), tx, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: policy });
    let mut errors = Vec::new();
    let mut done = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { ok: false, err, .. } => errors.push(err),
            OpMsg::TransferDone { ok, failed, skipped, cancelled, entry, .. } => done = Some((ok, failed, skipped, cancelled, entry.steps)),
            _ => {}
        }
    }
    let (ok, failed, skipped, was_cancelled, steps) = done.expect("a terminal line");
    assert!(!cancel.load(Ordering::Relaxed), "the flag was down again before later.txt, so only photo.png's error could stop the batch");
    assert_eq!((ok, failed, skipped, was_cancelled), (0, 1, 1, true), "photo.png's name no longer holds what it held, so it fails; later.txt is skipped");
    assert!(errors.len() == 1 && errors[0].starts_with("cancelled; the item it replaced is still in the Trash"), "{:?}", errors);
    assert!(!to.join("later.txt").exists() && text(&later) == "never reached", "the batch stopped before later.txt");
    assert!(matches!(&steps[..], [Step::Trashed(old)] if old.original == there), "undo can still restore it: {:?}", steps);
    assert!(!there.exists(), "the old item really is in the stand-in trash");
}

// The production shape: a real gio restore never lowers the user's Cancel, so the item is counted once, as failed.
#[test]
fn a_cancelled_replace_whose_put_back_fails_under_a_standing_cancel_counts_once_as_failed() {
    let d = TestDir::new("collide-cancel-standing");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let cancel = Arc::new(AtomicBool::new(false));
    let _trash = unrestorable_trash(&d, Arc::clone(&cancel), false);
    let (tx, rx) = channel();
    let policy = chosen("replace", asked(1, &[&photo], &to), &to);
    run_transfer_checked(1, false, owned(&[&photo]), to.clone(), Arc::clone(&cancel), tx, crate::backend::opsreq::TransferChecks { selection: None, destination: None, collision: policy });
    let mut errors = Vec::new();
    let mut done = None;
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { ok: false, err, .. } => errors.push(err),
            OpMsg::TransferDone { ok, failed, skipped, cancelled, .. } => done = Some((ok, failed, skipped, cancelled)),
            _ => {}
        }
    }
    assert!(cancel.load(Ordering::Relaxed), "the Cancel stood through the put-back, as it does with the real gio");
    assert_eq!(done.expect("a terminal line"), (0, 1, 0, true), "failed once and never also skipped");
    assert!(errors.len() == 1 && errors[0].starts_with("cancelled; the item it replaced is still in the Trash"), "{:?}", errors);
    assert!(!there.exists(), "the old item is in the stand-in trash, where undo can restore it");
}

#[test]
fn replace_refuses_an_item_holding_any_source_of_the_batch_in_either_order() {
    let d = TestDir::new("collide-holds-batch");
    let to = d.dir("d");
    let incoming = d.dir("e/x");
    d.file("e/x/new.txt", "new");
    d.dir("d/x");
    let inner = d.file("d/x/y", "inside the folder a replace would trash");
    let _trash = refusing_trash();
    let (a, b) = (incoming.as_path(), inner.as_path());
    for batch in [[a, b], [b, a]] {
        let (ok, _, _, errors, _) = transfer(false, &batch, &to, chosen("replace", asked(1, &batch, &to), &to));
        assert_eq!((ok, errors), (1, vec![HOLDS_SOURCE.to_string()]), "{:?}", batch);
        assert_eq!(text(&inner), "inside the folder a replace would trash");
        d.assert_contains(&to.join("y"));
        std::fs::remove_file(to.join("y")).unwrap();
    }
}

#[test]
fn replace_refuses_a_link_that_resolves_to_the_item_it_would_replace() {
    let d = TestDir::new("collide-link-target");
    let to = d.dir("to");
    d.dir("from");
    let report = d.file("to/report.pdf", "the only copy");
    // Through a second link, so only resolving it now, not its own text, finds the item already there.
    let chain = d.join("chain.pdf");
    std::os::unix::fs::symlink(&report, &chain).unwrap();
    let link = d.join("from/report.pdf");
    std::os::unix::fs::symlink(&chain, &link).unwrap();
    let _trash = refusing_trash();
    let (_, _, _, errors, _) = transfer(false, &[&link], &to, chosen("replace", asked(1, &[&link], &to), &to));
    assert_eq!(errors, vec![LINKS_HERE.to_string()], "trashing its target would leave a link to itself in its place");
    assert_eq!(text(&report), "the only copy");
    // A link at the name that the incoming link's text names is the same loop once the incoming one takes the name.
    let elsewhere = d.file("real.txt", "real");
    let named = to.join("named");
    std::os::unix::fs::symlink(&elsewhere, &named).unwrap();
    let naming = d.join("from/named");
    std::os::unix::fs::symlink(&named, &naming).unwrap();
    let (_, _, _, errors, _) = transfer(false, &[&naming], &to, chosen("replace", asked(2, &[&naming], &to), &to));
    assert_eq!(errors, vec![LINKS_HERE.to_string()]);
    assert_eq!(std::fs::read_link(&named).unwrap(), elsewhere);
}

#[test]
fn replace_refuses_a_link_whose_text_reaches_the_name_it_takes_through_any_link() {
    let d = TestDir::new("collide-link-walk");
    let to = d.dir("to");
    d.dir("from");
    let _trash = refusing_trash();
    // Relative text that chains back through another link already in dest.
    let report = d.file("to/report.pdf", "the only copy");
    std::os::unix::fs::symlink("report.pdf", to.join("alias")).unwrap();
    let chained = d.join("from/report.pdf");
    std::os::unix::fs::symlink("alias", &chained).unwrap();
    let (_, _, _, errors, _) = transfer(false, &[&chained], &to, chosen("replace", asked(1, &[&chained], &to), &to));
    assert_eq!(errors, vec![LINKS_HERE.to_string()], "report.pdf would read alias, which reads report.pdf");
    assert_eq!(text(&report), "the only copy");
    // The name holds a link to a folder, and the incoming text goes through that name to a file inside.
    let real = d.dir("real");
    d.file("real/file", "inside");
    std::os::unix::fs::symlink(&real, to.join("a")).unwrap();
    let through = d.join("from/a");
    std::os::unix::fs::symlink(to.join("a/file"), &through).unwrap();
    let (_, _, _, errors, _) = transfer(false, &[&through], &to, chosen("replace", asked(2, &[&through], &to), &to));
    assert_eq!(errors, vec![LINKS_HERE.to_string()], "a once it holds the link would be looked up inside itself");
    assert_eq!(std::fs::read_link(to.join("a")).unwrap(), real);
    // A lookup that cycles away from the name is broken like a dangling link, so it and one that ends elsewhere both reach the trash, which refuses.
    std::os::unix::fs::symlink("round", to.join("trip")).unwrap();
    std::os::unix::fs::symlink("trip", to.join("round")).unwrap();
    let endless = d.join("from/notes.txt");
    std::os::unix::fs::symlink("trip", &endless).unwrap();
    d.file("to/notes.txt", "there");
    let (_, _, _, errors, _) = transfer(false, &[&endless], &to, chosen("replace", asked(3, &[&endless], &to), &to));
    assert_eq!(errors, vec![TRASH_REFUSED.to_string()], "trip and round only reach each other, never notes.txt");
    let elsewhere = d.join("from/elsewhere.txt");
    std::os::unix::fs::symlink("alias", &elsewhere).unwrap();
    d.file("to/elsewhere.txt", "there");
    let (_, _, _, errors, _) = transfer(false, &[&elsewhere], &to, chosen("replace", asked(4, &[&elsewhere], &to), &to));
    assert_eq!(errors, vec![TRASH_REFUSED.to_string()], "alias ends at report.pdf, not at the name it takes");
}

#[test]
fn redo_of_a_replace_refuses_an_old_item_that_changed_since_the_undo() {
    let d = TestDir::new("collide-redo-changed");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let (_trash, can) = working_trash(&d);
    let (_, _, _, _, entry) = transfer(false, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&there);
    journal.undo().unwrap();
    std::fs::rename(&there, d.join("elsewhere.png")).unwrap();
    d.file("to/photo.png", "someone else's since");
    assert!(redo(&mut journal).unwrap_err().msg.contains("changed"));
    assert_eq!(text(&there), "someone else's since", "redo trashes only the item the undo put back");
    assert_eq!(std::fs::read_dir(&can).unwrap().count(), 0);
}

#[test]
fn a_link_at_the_name_is_replaced_as_itself_and_its_target_is_never_touched() {
    let d = TestDir::new("collide-link");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let link = to.join("photo.png");
    std::os::unix::fs::symlink(&photo, &link).unwrap();
    let (_trash, _can) = working_trash(&d);
    let (ok, _, _, errors, _) = transfer(false, &[&photo], &to, chosen("replace", asked(1, &[&photo], &to), &to));
    assert_eq!((ok, errors), (1, Vec::<String>::new()), "a link to the source does not hold the source");
    assert!(!link.symlink_metadata().unwrap().file_type().is_symlink(), "the link went to Trash and a copy took its name");
    assert_eq!((text(&link), text(&photo)), ("yours".to_string(), "yours".to_string()));
}

#[test]
fn a_failed_link_cleanup_with_a_replaced_entry_still_undoes_the_good_link() {
    use crate::backend::opsdispatch::{do_undo, test_fail_link_verify, test_reported_link, Ops};
    let d = TestDir::new("collide-link-leftover-undo");
    d.dir("src");
    let a = d.file("src/a.txt", "a-new");
    let b = d.file("src/b.txt", "b-new");
    let dest = d.dir("dest");
    d.file("dest/a.txt", "a-old");
    d.file("dest/b.txt", "b-old");
    // The second trash call arms the verify failure, so the first link lands and the second is left over.
    let can = d.dir("can");
    let at = can.clone();
    let calls = Rc::new(std::cell::Cell::new(0usize));
    let count = Rc::clone(&calls);
    trash::STAND_IN.with(|slot| *slot.borrow_mut() = Some(Rc::new(move |args: &[&str]| {
        if matches!(args, ["trash", "--", ..]) && count.get() == 1 {
            test_fail_link_verify(true);
        }
        if matches!(args, ["trash", "--", ..]) {
            count.set(count.get() + 1);
        }
        Some(gio_trash(&at, args))
    })));
    let _trash = StandIn;
    let (tx, _rx) = channel();
    let mut o = Ops::new(tx);
    o.question = Some(asked(1, &[&a, &b], &dest));
    let ask = Ask::parse(r#"{"c":"link","collide":"replace","collideId":1}"#);
    let mut buf = Vec::new();
    let paths = vec![a.to_string_lossy().to_string(), b.to_string_lossy().to_string()];
    let policy = ask.policy(o.question.take(), &dest).for_batch(&paths);
    test_reported_link(&mut o, &mut buf, "relative", paths, dest.clone(), policy);
    test_fail_link_verify(false);
    let line = String::from_utf8_lossy(&buf).to_string();
    assert!(line.contains(r#""t":"linked","ok":1,"failed":1,"skipped":0"#), "one link lands and one is left over: {}", line);
    // F24: the mixed answer carries the stranded sentence as a note.
    assert!(line.contains(r#""note":"#), "a stranded replace is named on the mixed answer: {}", line);
    assert!(line.contains(&format!("the link at {} could not be removed: ", crate::json::escape(&dest.join("b.txt").to_string_lossy()))), "the note names the leftover link: {}", line);
    assert!(line.contains("the item it replaced is still in the Trash"), "the note says the replaced item was not restored: {}", line);
    assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "a-new");
    let mut buf = Vec::new();
    do_undo(&mut buf, &mut o);
    let line = String::from_utf8_lossy(&buf).to_string();
    assert!(line.contains(r#""t":"undone","op":"link","ok":true"#), "the good link still reverses: {}", line);
    assert_eq!(std::fs::read_to_string(dest.join("a.txt")).unwrap(), "a-old");
    assert!(dest.join("b.txt").symlink_metadata().is_ok(), "the leftover link stays and is reported, never dropped");
    assert_eq!(std::fs::read_dir(&can).unwrap().count(), 2, "its original stays in the trash");
    // The note travels on first_err, which only an all-failed answer carries.
    let c = d.file("src/c.txt", "c-new");
    d.file("dest/c.txt", "c-old");
    o.question = Some(asked(2, &[&c], &dest));
    let ask = Ask::parse(r#"{"c":"link","collide":"replace","collideId":2}"#);
    test_fail_link_verify(true);
    let mut buf = Vec::new();
    let paths = vec![c.to_string_lossy().to_string()];
    let policy = ask.policy(o.question.take(), &dest).for_batch(&paths);
    test_reported_link(&mut o, &mut buf, "relative", paths, dest.clone(), policy);
    test_fail_link_verify(false);
    let line = String::from_utf8_lossy(&buf).to_string();
    assert!(line.contains(r#""where":"link""#), "an all-failed link is an error line: {}", line);
    assert!(line.contains("could not be removed: "), "the cleanup failure is named with its reason: {}", line);
    assert!(line.contains("the item it replaced is still in the Trash"), "the replaced item is not reported as restored: {}", line);
}

#[test]
fn a_missing_source_never_hides_a_stranded_replace_note() {
    // F25: an earlier batch failure must not hide the stranded link sentence.
    use crate::backend::opsdispatch::{test_fail_link_verify, test_reported_link, Ops};
    let d = TestDir::new("collide-link-stranded-first");
    d.dir("src");
    let c = d.file("src/c.txt", "c-new");
    let dest = d.dir("dest");
    d.file("dest/c.txt", "c-old");
    let (_trash, _can) = working_trash(&d);
    let (tx, _rx) = channel();
    let mut o = Ops::new(tx);
    o.question = Some(asked(7, &[&c], &dest));
    let ask = Ask::parse(r#"{"c":"link","collide":"replace","collideId":7}"#);
    let gone = d.path().join("src/x.txt").to_string_lossy().to_string();
    test_fail_link_verify(true);
    let mut buf = Vec::new();
    let paths = vec![gone, c.to_string_lossy().to_string()];
    let policy = ask.policy(o.question.take(), &dest).for_batch(&paths);
    test_reported_link(&mut o, &mut buf, "relative", paths, dest.clone(), policy);
    test_fail_link_verify(false);
    let line = String::from_utf8_lossy(&buf).to_string();
    assert!(line.contains(r#""where":"link""#), "an all-failed link is an error line: {}", line);
    assert!(line.contains("x.txt"), "the first failure is still named: {}", line);
    assert!(line.contains("c.txt"), "the stranded link is named too: {}", line);
    assert!(line.contains("the item it replaced is still in the Trash"), "the replaced item is not reported as restored: {}", line);
}
