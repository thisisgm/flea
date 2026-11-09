// The question and the choices that need no trash, driven against real files in a sandbox.
use super::*;
use crate::backend::opsdispatch::report_op;
use crate::backend::opsreq::{run_transfer_checked, spawn_total, OpMsg};
use crate::backend::testdir::TestDir;
use crate::backend::trash;
use crate::backend::undo::{Entry, Journal};
use std::process::Output;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

// Far past one question over a handful of files, and short enough to fail a stuck one.
const WAIT: Duration = Duration::from_secs(5);
// How often a test looks again for the sweep's total.
const POLL: Duration = Duration::from_millis(5);

// Clears this thread's stand-in on the way out, so no later test on the thread inherits it.
pub(crate) struct StandIn;

impl Drop for StandIn {
    fn drop(&mut self) {
        trash::STAND_IN.with(|slot| *slot.borrow_mut() = None);
    }
}

// A trash that takes nothing, the way gio answers on a mount with no trash of its own.
pub(crate) fn refusing_trash() -> StandIn {
    trash::STAND_IN.with(|slot| *slot.borrow_mut() = Some(Rc::new(|_: &[&str]| -> Option<Output> { None })));
    StandIn
}

pub(crate) fn text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

pub(crate) fn owned(paths: &[&Path]) -> Vec<String> {
    paths.iter().map(|p| p.to_string_lossy().to_string()).collect()
}

pub(crate) fn asked(id: usize, paths: &[&Path], dest: &Path) -> Question {
    ask(id, &owned(paths), &dest.to_string_lossy(), None).0
}

// Asks on the question's own thread and lands the answer the way the loop does.
pub(super) fn answered(ops: &mut Ops, rx: &Receiver<OpMsg>, id: usize, menu_id: usize, named: Vec<String>, dest: &str) -> String {
    ask_beside(ops, id, menu_id, named, dest, None, (&Arc::new(Db::load()), &Arc::new(Names::load())));
    written(ops, rx)
}

// Hands the next answer to the loop's own report_op, skipping any other line on the channel, and answers what it wrote.
fn written(ops: &mut Ops, rx: &Receiver<OpMsg>) -> String {
    loop {
        let msg = rx.recv_timeout(WAIT).expect("a collisions answer");
        if matches!(msg, OpMsg::Asked { .. }) {
            let mut out = Vec::new();
            report_op(&mut out, ops, msg);
            return String::from_utf8_lossy(&out).trim_end().to_string();
        }
    }
}

// Holds the next question's thread until the sender it answers is sent to or dropped.
fn hold_next_question() -> std::sync::mpsc::Sender<()> {
    let (release, hold) = channel();
    HOLD.with(|slot| *slot.borrow_mut() = Some(hold));
    release
}

// The batch total the sweep beside a transfer settles on, which it publishes once and only when it counted everything.
fn swept(paths: &[String], dest: &Path, policy: &Policy) -> u64 {
    let (cancel, settled, sweeping) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicBool::new(true)));
    spawn_total(paths, dest, policy.skipping(), &cancel, &settled, &sweeping);
    let started = Instant::now();
    while settled.load(Ordering::Relaxed) == 0 && started.elapsed() < WAIT {
        std::thread::sleep(POLL);
    }
    sweeping.store(false, Ordering::Relaxed);
    settled.load(Ordering::Relaxed)
}

pub(crate) fn chosen(word: &str, question: Question, dest: &Path) -> Policy {
    let id = question.id;
    Ask { word: Some(word.to_string()), id, inner: None, fat: None }.policy(Some(question), dest)
}

// Runs one transfer on this thread and answers its counts, the per-item errors and the journal entry.
pub(crate) fn transfer(moving: bool, paths: &[&Path], dest: &Path, policy: Policy) -> (usize, usize, usize, Vec<String>, Entry) {
    let (tx, rx) = channel();
    run_transfer_checked(1, moving, owned(paths), dest.to_path_buf(), Arc::new(AtomicBool::new(false)), tx, None, None, policy);
    let mut errors = Vec::new();
    for msg in rx.iter() {
        match msg {
            OpMsg::Item { ok: false, err, .. } => errors.push(err),
            OpMsg::TransferDone { ok, failed, skipped, entry, .. } => return (ok, failed, skipped, errors, entry),
            _ => {}
        }
    }
    panic!("no terminal line");
}

#[test]
fn the_question_lists_only_sources_whose_name_the_destination_holds() {
    let d = TestDir::new("collide-ask");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    d.file("to/photo.png", "there");
    let free = d.file("from/notes.txt", "nothing there");
    let album = d.dir("from/album");
    d.dir("to/album");
    let local = d.file("to/local.txt", "already in the destination");
    let gone = d.join("from/gone.txt");
    d.file("to/gone.txt", "its name is taken, so only the vanished source keeps it out");
    let (question, shown) = ask(7, &owned(&[&photo, &free, &album, &local, &gone]), &to.to_string_lossy(), None);
    let names: Vec<_> = shown.iter().map(|s| (s.name.as_str(), s.dir)).collect();
    assert_eq!(names, vec![("photo.png", false), ("album", true)], "a free name, an item already there and a vanished source never ask");
    assert_eq!(question.seen.len(), 2);
    assert!(question.seen[&photo].same_item(&ItemIdentity::inspect(&to.join("photo.png")).unwrap()));
}

#[test]
fn the_answer_names_three_and_counts_every_collision() {
    let d = TestDir::new("collide-answer");
    let to = d.dir("to");
    d.dir("from");
    let mut sources = Vec::new();
    for name in ["a", "b", "c", "d", "e"] {
        sources.push(d.dir(&format!("from/{}", name)));
        d.file(&format!("to/{}", name), "there");
    }
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let refs: Vec<&Path> = sources.iter().map(|p| p.as_path()).collect();
    let line = answered(&mut ops, &rx, 9, 0, owned(&refs), &to.to_string_lossy());
    assert!(line.starts_with(r#"{"t":"collisions","id":9,"total":5,"names":[{"n":"a","d":true,"i":"folder"},"#), "{}", line);
    assert_eq!(line.matches(r#""n":"#).count(), SHOWN, "the card lists three and says how many more");
    assert!(ops.question.is_some(), "the transfer that follows needs what the question saw");
    // A relative dest resolves against the working directory, the crate root under cargo, where Cargo.toml would collide.
    let manifest = d.file("from/Cargo.toml", "yours");
    assert!(Path::new("Cargo.toml").is_file(), "the working directory is not the crate root, so this check proves nothing");
    let empty = answered(&mut ops, &rx, 10, 0, owned(&[&manifest]), ".");
    assert_eq!(empty, r#"{"t":"collisions","id":10,"total":0,"names":[],"mergeable":false}"#, "a relative destination asks nothing");
    let file = answered(&mut ops, &rx, 11, 0, owned(&refs), &to.join("a").to_string_lossy());
    assert!(file.contains(r#""total":0"#), "a destination that is a file asks nothing either");
}

#[test]
fn the_question_is_asked_beside_the_loop_and_only_the_latest_one_is_kept_and_answered() {
    let d = TestDir::new("collide-latest");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    d.file("to/photo.png", "there");
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    let dest = to.to_string_lossy().to_string();
    let (db, names) = (Arc::new(Db::load()), Arc::new(Names::load()));
    let release = hold_next_question();
    ask_beside(&mut ops, 1, 0, owned(&[&photo]), &dest, None, (&db, &names));
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)), "the loop is back while its question is still held on a thread of its own");
    assert!(ops.question.is_none(), "nothing is kept until an answer lands on the loop");
    ask_beside(&mut ops, 2, 0, owned(&[&photo]), &dest, None, (&db, &names));
    let later = written(&mut ops, &rx);
    release.send(()).unwrap();
    let earlier = written(&mut ops, &rx);
    assert!(later.starts_with(r#"{"t":"collisions","id":2,"total":1"#), "{}", later);
    assert_eq!(earlier, "", "the earlier question landing last is dropped, never answered after the later one");
    assert_eq!(ops.question.as_ref().map(|q| q.id), Some(2), "and it does not replace the later one");
}

#[test]
fn the_choice_word_parses_and_anything_else_refuses() {
    let ask = Ask::parse(r#"{"c":"transfer","dest":"/x","collide":"replace","collideId":7}"#);
    assert_eq!(ask, Ask { word: Some("replace".into()), id: 7, inner: None, fat: None });
    assert_eq!(Ask::parse(r#"{"c":"transfer","dest":"/x"}"#), Ask::default(), "absent is today's transfer");
    assert_eq!(Collide::from_word("overwrite"), Collide::Refuse, "a word this wire does not define can never replace");
    assert_eq!(Collide::from_word("keep"), Collide::Keep);
    assert_eq!(Collide::from_word("skip"), Collide::Skip);
}

#[test]
fn keep_both_names_the_incoming_item_as_duplicate_does_and_undo_removes_only_it() {
    let d = TestDir::new("collide-keep");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    d.file("to/photo.png", "there");
    let (ok, failed, _, _, entry) = transfer(false, &[&photo], &to, chosen("keep", asked(1, &[&photo], &to), &to));
    assert_eq!((ok, failed), (1, 0));
    assert_eq!(text(&to.join("photo copy.png")), "yours");
    assert_eq!(text(&to.join("photo.png")), "there", "keep both never touches the item already there");
    let (ok, _, _, _, _) = transfer(false, &[&photo], &to, chosen("keep", asked(2, &[&photo], &to), &to));
    assert_eq!(ok, 1);
    assert_eq!(text(&to.join("photo copy 2.png")), "yours", "the next free name, the way Duplicate steps");
    let mut journal = Journal::new();
    journal.push(entry);
    d.assert_contains(&to.join("photo copy.png"));
    journal.undo().unwrap();
    assert!(!to.join("photo copy.png").exists());
    assert_eq!(text(&to.join("photo.png")), "there");
}

#[test]
fn skip_leaves_every_colliding_name_and_counts_it_as_skipped() {
    let d = TestDir::new("collide-skip");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let notes = d.file("from/notes.txt", "notes");
    d.file("to/photo.png", "there");
    let question = asked(1, &[&photo, &notes], &to);
    let (ok, failed, skipped, errors, _) = transfer(false, &[&photo, &notes], &to, chosen("skip", question, &to));
    assert_eq!((ok, failed, skipped), (1, 0, 1), "the rest is copied and the collision is counted, not failed");
    assert!(errors.is_empty());
    assert_eq!(text(&to.join("photo.png")), "there");
    assert_eq!(text(&to.join("notes.txt")), "notes");
}

#[test]
fn a_name_that_appears_after_the_question_is_refused_whatever_the_choice() {
    let d = TestDir::new("collide-race");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let _trash = refusing_trash();
    for word in ["keep", "replace", "skip", "refuse"] {
        let question = asked(1, &[&photo], &to);
        d.assert_contains(&to.join("photo.png"));
        std::fs::write(to.join("photo.png"), "arrived after the question").unwrap();
        let (ok, failed, _, errors, entry) = transfer(false, &[&photo], &to, chosen(word, question, &to));
        assert_eq!((ok, failed, errors.as_slice()), (0, 1, &["already exists".to_string()][..]), "{}", word);
        assert!(entry.steps.is_empty(), "{}", word);
        assert_eq!(text(&to.join("photo.png")), "arrived after the question", "{}", word);
        std::fs::remove_file(to.join("photo.png")).unwrap();
    }
    // The name the question saw, swapped for another item since, is a new name to this rule.
    d.file("to/photo.png", "the one asked about");
    let question = asked(2, &[&photo], &to);
    std::fs::rename(to.join("photo.png"), d.join("moved-away.png")).unwrap();
    d.file("to/photo.png", "a different item under the same name");
    let refused = ["already exists".to_string()];
    let (_, failed, _, errors, entry) = transfer(false, &[&photo], &to, chosen("replace", question, &to));
    assert_eq!((failed, errors.as_slice(), entry.steps.len()), (1, &refused[..], 0), "the exclusive create refused it, no trash was tried");
    assert_eq!(text(&to.join("photo.png")), "a different item under the same name");
    // A transfer naming another question, or none, covers nothing.
    let stale = asked(3, &[&photo], &to);
    let other = Ask { word: Some("replace".into()), id: 4, inner: None, fat: None }.policy(Some(stale), &to);
    let (_, failed, _, errors, entry) = transfer(false, &[&photo], &to, other);
    assert_eq!((failed, errors.as_slice(), entry.steps.len()), (1, &refused[..], 0), "another question's id covers nothing");
    let (_, failed, _, errors, _) = transfer(false, &[&photo], &to, Policy::default());
    assert_eq!((failed, errors.as_slice()), (1, &refused[..]), "no choice at all is today's refusal");
}

#[test]
fn the_batch_total_leaves_out_the_items_skip_leaves_in_place() {
    let d = TestDir::new("collide-counted");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "a photo, longer than the notes");
    let notes = d.file("from/notes.txt", "notes");
    let there = d.file("to/photo.png", "there");
    let paths = owned(&[&photo, &notes]);
    let (photo_bytes, notes_bytes) = (text(&photo).len() as u64, text(&notes).len() as u64);
    let skip = chosen("skip", asked(1, &[&photo, &notes], &to), &to);
    assert_eq!(swept(&paths, &to, &skip), notes_bytes, "the skipped photo copies nothing, so it adds nothing to the time left");
    let keep = chosen("keep", asked(2, &[&photo, &notes], &to), &to);
    assert_eq!(swept(&paths, &to, &keep), photo_bytes + notes_bytes, "every other choice moves every item");
    // A name emptied since the question lands its item after all, so the total counts it by place's own test.
    let skip = chosen("skip", asked(3, &[&photo, &notes], &to), &to);
    d.assert_contains(&there);
    std::fs::remove_file(&there).unwrap();
    assert_eq!(swept(&paths, &to, &skip), photo_bytes + notes_bytes, "the photo will be copied, so its bytes are counted");
    let (ok, _, skipped, _, _) = transfer(false, &[&photo, &notes], &to, skip);
    assert_eq!((ok, skipped), (2, 0), "and it was");
}

#[test]
fn an_item_gone_since_the_question_is_simply_copied_and_a_vanished_destination_is_refused() {
    let d = TestDir::new("collide-vanished");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    let there = d.file("to/photo.png", "there");
    let _trash = refusing_trash();
    let question = asked(1, &[&photo], &to);
    d.assert_contains(&there);
    std::fs::remove_file(&there).unwrap();
    let (ok, _, _, _, entry) = transfer(false, &[&photo], &to, chosen("replace", question, &to));
    assert_eq!(ok, 1, "nothing to trash, so the item lands under its own name");
    assert!(matches!(&entry.steps[..], [Step::Copied { .. }]));
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx);
    answered(&mut ops, &rx, 5, 0, owned(&[&photo]), &to.to_string_lossy());
    d.assert_contains(&to);
    std::fs::remove_dir_all(&to).unwrap();
    let mut out = Vec::new();
    let ask = Ask { word: Some("replace".into()), id: 5, inner: None, fat: None };
    crate::backend::opsdispatch::start_transfer(&mut out, &mut ops, "copy", owned(&[&photo]), &to.to_string_lossy(), ask);
    let line = String::from_utf8_lossy(&out).to_string();
    assert!(line.contains(r#""t":"error","where":"transfer""#) && line.contains("not found"), "{}", line);
    assert!(!to.exists(), "a transfer never creates its destination");
}

#[test]
fn a_copy_into_its_own_folder_keeps_both_and_a_move_onto_itself_does_nothing() {
    let d = TestDir::new("collide-here");
    let photo = d.file("photo.png", "yours");
    let refuse = || Ask { word: Some("refuse".into()), id: 0, inner: None, fat: None }.policy(None, d.path());
    let (ok, _, _, _, _) = transfer(false, &[&photo], d.path(), refuse());
    assert_eq!(ok, 1);
    assert_eq!(text(&d.join("photo copy.png")), "yours", "Duplicate's own name, never a question");
    let (ok, failed, skipped, errors, entry) = transfer(true, &[&photo], d.path(), refuse());
    assert_eq!((ok, failed, skipped), (0, 0, 1), "a move onto itself is no error and no work");
    assert!(errors.is_empty() && entry.steps.is_empty());
    assert_eq!(text(&photo), "yours");
    let (_, failed, _, errors, _) = transfer(false, &[&photo], d.path(), Policy::default());
    assert_eq!((failed, errors), (1, vec![ALREADY_THERE.to_string()]), "an older client's request is refused exactly as before");
}

// Copy to closes its dialog, which expires the menu's live selection, before the answer to its question lands.
#[test]
fn a_menu_question_captures_the_selection_its_transfer_runs_on_after_the_menu_closes() {
    use crate::backend::menu_actions::MenuActions;
    let d = TestDir::new("collide-menu");
    let to = d.dir("to");
    d.dir("from");
    let photo = d.file("from/photo.png", "yours");
    d.file("to/photo.png", "there");
    let (tx, rx) = channel();
    let mut ops = Ops::new(tx.clone());
    let menu = MenuActions::new(tx);
    menu.request(r#"{"op":"snapshot","id":5}"#.into(), owned(&[&photo]), None);
    let OpMsg::Meta { line } = rx.recv_timeout(WAIT).unwrap() else { panic!("snapshot reply") };
    assert!(line.contains(r#""ok":true"#), "{}", line);
    ops.menuactions = Some(menu);
    let dest = to.to_string_lossy().to_string();
    let release = hold_next_question();
    ask_beside(&mut ops, 7, 5, Vec::new(), &dest, None, (&Arc::new(Db::load()), &Arc::new(Names::load())));
    // The very next request, sent while the question's thread has not looked at anything yet.
    ops.menuactions.as_ref().unwrap().request(r#"{"op":"close","id":5}"#.into(), Vec::new(), None);
    release.send(()).unwrap();
    let asked = written(&mut ops, &rx);
    assert!(asked.contains(r#""total":1,"names":[{"n":"photo.png""#), "the menu's own selection is what is asked about: {}", asked);
    let mut out = Vec::new();
    crate::backend::opsdispatch::start_menu_transfer(&mut out, &mut ops, "copy", 5, &dest, Ask { word: Some("keep".into()), id: 99, inner: None, fat: None });
    assert!(String::from_utf8_lossy(&out).contains(crate::backend::menu_actions::SELECTION_CHANGED), "a transfer naming another question gets no capture");
    out.clear();
    crate::backend::opsdispatch::start_menu_transfer(&mut out, &mut ops, "copy", 5, &dest, Ask { word: Some("keep".into()), id: 7, inner: None, fat: None });
    assert!(String::from_utf8_lossy(&out).contains(r#""t":"transferstarted""#), "{}", String::from_utf8_lossy(&out));
    let mut done = None;
    while done.is_none() {
        let message = rx.recv_timeout(WAIT).expect("a terminal line");
        if let OpMsg::TransferDone { ok, failed, .. } = &message {
            done = Some((*ok, *failed));
        }
        crate::backend::opsdispatch::report_op(&mut out, &mut ops, message);
    }
    assert_eq!(done, Some((1, 0)));
    assert_eq!(text(&to.join("photo copy.png")), "yours", "Keep both on Copy to names the copy as Duplicate does");
    assert_eq!(text(&to.join("photo.png")), "there");
    let expired = answered(&mut ops, &rx, 8, 5, Vec::new(), &dest);
    assert!(expired.contains(r#""total":0"#), "an expired selection asks nothing: {}", expired);
}

#[test]
fn a_folder_resolved_once_decides_every_source_in_it_like_a_fresh_resolve() {
    let d = TestDir::new("already-there-parents");
    let real = d.dir("real");
    let elsewhere = d.dir("elsewhere");
    d.file("real/a.txt", "a");
    d.file("elsewhere/a.txt", "a");
    let link = d.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let dest_real = real.canonicalize().unwrap();
    let mut parents = Parents::default();
    // The same name through the link and straight, and a twin in another folder, each asked twice so the second answer is the cached one.
    for src in [link.join("a.txt"), real.join("a.txt"), elsewhere.join("a.txt"), link.join("a.txt"), elsewhere.join("a.txt")] {
        let dst = real.join("a.txt");
        assert_eq!(already_there_in(&src, "a.txt", &dst, &dest_real, &mut parents), already_there(&src, "a.txt", &dst, &dest_real), "{}", src.display());
    }
    assert!(already_there_in(&link.join("a.txt"), "a.txt", &real.join("a.txt"), &dest_real, &mut parents), "through a symlinked folder it is the item itself");
    assert!(!already_there_in(&elsewhere.join("a.txt"), "a.txt", &real.join("a.txt"), &dest_real, &mut parents));
}

#[path = "collide_fat_tests.rs"]
mod fat_tests;
#[path = "collide_table_tests.rs"]
mod table_tests;
#[path = "collide_twins_tests.rs"]
mod twins_tests;
