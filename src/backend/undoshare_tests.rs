// Two journals in one test process pointed at one temp runtime dir, never at the operator's own.
use super::{Shared, JOURNAL_FILE};
use crate::backend::testdir::TestDir;
use crate::backend::undo::{Entry, ItemIdentity, Journal, Step};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;

fn runtime(sandbox: &TestDir) -> PathBuf {
    sandbox.path().join("runtime")
}

// Pre-created with the mode the validator demands, so the test exercises the file, not the fallback.
fn runtime_0700(sandbox: &TestDir) -> PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let dir = runtime(sandbox);
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    dir
}

fn shared_journal(sandbox: &TestDir) -> Journal {
    let mut journal = Journal::new();
    journal.attach_test_shared(runtime(sandbox));
    journal
}

fn rename_steps(sandbox: &TestDir, from: &str, to: &str) -> (PathBuf, PathBuf, Vec<Step>) {
    let from_path = sandbox.path().join(from);
    std::fs::write(&from_path, "payload").unwrap();
    let to_path = sandbox.path().join(to);
    for path in [&from_path, &to_path] {
        sandbox.assert_contains(path);
    }
    let (dst, steps) = crate::backend::ops::rename(&from_path, to).unwrap();
    assert_eq!(dst, to_path);
    (from_path, to_path, steps)
}

// One MiB past the journal cap, so the op string alone makes the entry too big.
const TOO_BIG: usize = super::MAX_FILE_BYTES as usize + 1024 * 1024;
const BARRIER_MSG: &str = "That operation was too large to undo.";
fn huge_entry(path: PathBuf) -> Entry {
    Entry { op: "h".repeat(TOO_BIG), steps: vec![Step::Created { path }] }
}

#[test]
fn an_entry_recorded_by_a_is_undone_by_b() {
    let sandbox = TestDir::new("xundo-ab");
    let (from, to, steps) = rename_steps(&sandbox, "a.txt", "b.txt");
    let mut a = shared_journal(&sandbox);
    a.push(Entry { op: "rename".to_string(), steps });
    assert!(to.exists());
    let mut b = shared_journal(&sandbox);
    assert_eq!(b.undo().unwrap(), "rename");
    assert!(from.exists(), "B undid A's rename");
    assert!(!to.exists());
    assert_eq!(std::fs::read_to_string(&from).unwrap(), "payload");
    // Spent in both: a second undo anywhere finds nothing.
    assert!(b.undo().is_err());
    assert!(a.undo().is_err());
}

#[test]
fn concurrent_takes_never_take_one_entry_twice() {
    let sandbox = TestDir::new("xundo-race");
    let count = 8;
    let mut maker = shared_journal(&sandbox);
    for index in 0..count {
        let (_, _, steps) = rename_steps(&sandbox, &format!("f{}.txt", index), &format!("g{}.txt", index));
        maker.push(Entry { op: "rename".to_string(), steps });
    }
    let dir = runtime(&sandbox);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    let mut handles = Vec::new();
    for _ in 0..4 {
        let dir = dir.clone();
        let gate = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let mut journal = Journal::new();
            journal.attach_test_shared(dir);
            gate.wait();
            let mut taken = 0;
            loop {
                match journal.undo() {
                    Ok(op) => {
                        assert_eq!(op, "rename");
                        taken += 1;
                    }
                    Err(e) if e.msg == "there is nothing to undo" => break,
                    Err(e) => panic!("unexpected undo error: {} {}", e.where_, e.msg),
                }
            }
            taken
        }));
    }
    let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert_eq!(total, count, "every entry undone exactly once across four journals");
    for index in 0..count {
        assert!(sandbox.path().join(format!("f{}.txt", index)).exists());
        assert!(!sandbox.path().join(format!("g{}.txt", index)).exists());
    }
}

#[test]
fn a_symlinked_runtime_dir_is_refused_and_falls_back() {
    let sandbox = TestDir::new("xundo-link");
    let target = sandbox.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let link = sandbox.path().join("runtime");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert!(Shared::at(link.clone()).is_none(), "a symlinked dir is refused");
    let mut journal = Journal::new();
    journal.attach_test_shared(link);
    // A refused dir falls back to memory: the entry undoes locally, nothing writes through the link.
    let (_, to, steps) = rename_steps(&sandbox, "a.txt", "b.txt");
    journal.push(Entry { op: "rename".to_string(), steps });
    assert_eq!(journal.undo().unwrap(), "rename");
    assert!(!to.exists());
    assert_eq!(std::fs::read_dir(&target).unwrap().count(), 0, "nothing was written through the link");
}

#[test]
fn a_group_accessible_runtime_dir_is_refused_and_falls_back() {
    let sandbox = TestDir::new("xundo-mode");
    let dir = runtime(&sandbox);
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Shared::at(dir.clone()).is_none(), "a 0755 dir is refused");
    let mut journal = Journal::new();
    journal.attach_test_shared(dir.clone());
    let (_, to, steps) = rename_steps(&sandbox, "a.txt", "b.txt");
    journal.push(Entry { op: "rename".to_string(), steps });
    assert_eq!(journal.undo().unwrap(), "rename");
    assert!(!to.exists());
    assert!(!dir.join(JOURNAL_FILE).exists(), "a refused dir gains no journal file");
}

#[test]
fn an_oversized_file_is_ignored() {
    let sandbox = TestDir::new("xundo-huge");
    let dir = runtime_0700(&sandbox);
    let big = vec![b'x'; TOO_BIG];
    std::fs::write(dir.join(JOURNAL_FILE), &big).unwrap();
    let mut journal = shared_journal(&sandbox);
    assert!(journal.undo().is_err(), "an oversized file is ignored, not trusted");
    // The next push remains in memory and leaves the oversized document intact.
    let (_, to, steps) = rename_steps(&sandbox, "a.txt", "b.txt");
    journal.push(Entry { op: "rename".to_string(), steps });
    assert_eq!(std::fs::read(dir.join(JOURNAL_FILE)).unwrap(), big);
    assert_eq!(journal.undo().unwrap(), "rename");
    assert!(!to.exists());
    drop(big);
}

#[test]
fn a_planted_symlink_at_the_journal_path_is_left_unchanged() {
    let sandbox = TestDir::new("xundo-plant");
    let dir = runtime_0700(&sandbox);
    let outside = sandbox.path().join("outside.txt");
    std::fs::write(&outside, "untouched").unwrap();
    std::os::unix::fs::symlink(&outside, dir.join(JOURNAL_FILE)).unwrap();
    let mut journal = shared_journal(&sandbox);
    journal.push(Entry { op: "rename".to_string(), steps: vec![Step::Created { path: sandbox.path().join("x") }] });
    assert!(dir.join(JOURNAL_FILE).symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "untouched");
}

#[test]
fn an_endless_journal_is_ignored_within_its_bound() {
    let sandbox = TestDir::new("xundo-zero");
    let dir = runtime_0700(&sandbox);
    std::os::unix::fs::symlink("/dev/zero", dir.join(JOURNAL_FILE)).unwrap();
    let mut journal = shared_journal(&sandbox);
    let ignored = crate::backend::fifotest::within("an endless journal", move || journal.undo().is_err());
    assert!(ignored, "an endless journal is ignored, not trusted");
}

#[test]
fn a_fifo_journal_is_ignored_within_its_bound() {
    let sandbox = TestDir::new("xundo-fifo");
    let dir = runtime_0700(&sandbox);
    crate::backend::fifotest::mkfifo(&dir.join(JOURNAL_FILE));
    let mut journal = shared_journal(&sandbox);
    let ignored = crate::backend::fifotest::within("a fifo journal", move || journal.undo().is_err());
    assert!(ignored, "a fifo journal is ignored, not trusted");
}

#[test]
fn the_journal_holds_fifty_entries_oldest_dropped() {
    let sandbox = TestDir::new("xundo-cap");
    let mut journal = shared_journal(&sandbox);
    for index in 0..55 {
        journal.push(Entry { op: format!("op-{}", index), steps: vec![Step::Created { path: sandbox.path().join(format!("f{}", index)) }] });
    }
    let text = std::fs::read_to_string(runtime(&sandbox).join(JOURNAL_FILE)).unwrap();
    let doc = crate::backend::undocodec::decode(&text).expect("the journal file decodes");
    assert_eq!(doc.undo.len(), 50);
    assert_eq!(doc.undo.first().unwrap().op, "op-5", "the oldest entries went first");
    assert_eq!(doc.undo.last().unwrap().op, "op-54");
}

#[test]
fn an_empty_push_writes_nothing() {
    let sandbox = TestDir::new("xundo-empty");
    let mut journal = shared_journal(&sandbox);
    journal.push(Entry { op: "rename".to_string(), steps: Vec::new() });
    assert!(!runtime(&sandbox).join(JOURNAL_FILE).exists(), "a no-op records nothing, not even a file");
}

#[test]
fn identity_check_still_refuses_a_replaced_file() {
    let sandbox = TestDir::new("xundo-identity");
    let (from, to, steps) = rename_steps(&sandbox, "a.txt", "b.txt");
    let mut a = shared_journal(&sandbox);
    a.push(Entry { op: "rename".to_string(), steps });
    // Someone else's file now lives at the new name, so the undo must refuse, not remove it.
    sandbox.replace_file(&to, "someone else");
    let mut b = shared_journal(&sandbox);
    let error = b.undo().unwrap_err();
    assert!(error.msg.contains("replaced"), "unexpected refusal: {}", error.msg);
    assert_eq!(std::fs::read_to_string(&to).unwrap(), "someone else");
    assert!(!from.exists());
}

#[test]
fn redo_follows_the_same_rule_across_journals() {
    let sandbox = TestDir::new("xundo-redo");
    let (from, to, steps) = rename_steps(&sandbox, "a.txt", "b.txt");
    let mut a = shared_journal(&sandbox);
    a.push(Entry { op: "rename".to_string(), steps });
    let mut b = shared_journal(&sandbox);
    assert_eq!(b.undo().unwrap(), "rename");
    assert!(from.exists());
    let (tx, _rx) = channel();
    assert_eq!(a.redo(1, &AtomicBool::new(false), &tx).unwrap(), "rename");
    assert!(to.exists(), "A redid the entry B undid");
    assert!(!from.exists());
    // And back once more from the other side, so the redo stack itself is shared.
    assert_eq!(b.undo().unwrap(), "rename");
    assert!(from.exists());
}

#[test]
fn every_step_kind_round_trips_with_exact_integers() {
    let sandbox = TestDir::new("xundo-codec");
    let dir = sandbox.path();
    // Past f64's exact range, so a float-parsed number would come back a different file.
    let id = ItemIdentity::from_parts(u64::MAX, 1 << 60, 0o100000, 7, (1 << 62, -3), (0, 1 << 62), Some((1 << 62, u32::MAX)));
    let unborn = ItemIdentity::from_parts(u64::MAX, 1 << 60, 0o100000, 7, (1 << 62, -3), (0, 1 << 62), None);
    let entry = Entry {
        op: "mixed".to_string(),
        steps: vec![
            Step::Moved { from: dir.join("a"), to: dir.join("b"), before: id.clone(), after: id.clone(), rename_fat: false },
            Step::Created { path: dir.join("c") },
            Step::Linked { path: dir.join("d"), identity: id.clone(), source: dir.join("e"),
                kind: crate::backend::link::LinkKind::Hard },
            Step::Copied { from: dir.join("f"), to: dir.join("g"), source: id.clone(), created: id.clone(), manifest: None, manifest_nonce: None, merge: false, rename_fat: false },
            Step::MadeDir { path: dir.join("h"), identity: id.clone() },
            Step::MadeFile { path: dir.join("i"), identity: unborn.clone() },
            Step::Trashed(crate::backend::trash::Entry { original: dir.join("j"), uri: "trash:///j".to_string(), before: Vec::new(), trashed_at: String::new() }),
            Step::Mode { path: dir.join("k"), before: 0o644, after: 0o600, dev: u64::MAX, ino: 1 << 60, born: Some((1 << 62, u32::MAX)), generation: Some(u64::MAX) },
            Step::Mode { path: dir.join("l"), before: 0o644, after: 0o600, dev: u64::MAX, ino: 1 << 60, born: None, generation: None },
            Step::Barrier,
        ],
    };
    let doc = super::Doc {
        undo: vec![entry],
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    };
    let text = crate::jsondoc::render(&crate::backend::undocodec::encode(&doc));
    assert!(text.contains("\"born\": null"), "an unknown birth time is encoded as null");
    assert!(text.contains("\"b\": null"), "and so is an identity's");
    let back = crate::backend::undocodec::decode(&text).expect("the codec reads what it wrote");
    assert!(matches!(&back.undo[0].steps[0], Step::Moved { before, .. } if before.born() == Some((1 << 62, u32::MAX))),
        "an identity's birth time comes back exact");
    assert!(matches!(&back.undo[0].steps[5], Step::MadeFile { identity, .. } if identity.born().is_none()),
        "and an unknown one stays unknown");
    assert_eq!(crate::jsondoc::render(&crate::backend::undocodec::encode(&back)), text,
        "a second render is byte-identical, so every integer survived exactly");
    assert!(text.contains(&u64::MAX.to_string()), "the huge device number is really in the file");
}

#[test]
fn a_trash_entry_with_no_uri_is_a_record_not_a_defect() {
    // An empty uri is a record: the codec reads it back instead of ignoring the whole file.
    let sandbox = TestDir::new("xundo-nouri");
    let doc = super::Doc {
        undo: vec![Entry {
            op: "trash".to_string(),
            steps: vec![Step::Trashed(crate::backend::trash::Entry::unresolved(sandbox.path().join("gone"), Vec::new(), String::new()))],
        }],
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    };
    let text = crate::jsondoc::render(&crate::backend::undocodec::encode(&doc));
    let mut back = crate::backend::undocodec::decode(&text).expect("an empty uri decodes");
    assert_eq!(back.undo.len(), 1);
    let mut journal = shared_journal(&sandbox);
    journal.push(Entry { op: "trash".to_string(), steps: back.undo.pop().unwrap().steps });
    assert!(journal.undo().unwrap_err().msg.contains("cannot be restored"));
}

#[test]
fn foreign_records_are_never_trusted() {
    for (tag, body) in [
        ("version", r#"{"v":4,"undo":[],"redo":[]}"#),
        ("relative", r#"{"v":1,"undo":[{"op":"rename","steps":[{"k":"c","path":"rel/x"}]}],"redo":[]}"#),
        ("kind", r#"{"v":1,"undo":[{"op":"rename","steps":[{"k":"mf","path":"/x","id":{"d":1,"i":2,"k":511,"l":0,"m":[0,0],"c":[0,0]}}]}],"redo":[]}"#),
        ("shape", r#"{"v":1,"undo":[{"op":"rename","steps":[{"k":"m","from":"/a"}]}],"redo":[]}"#),
        ("born", r#"{"v":2,"undo":[{"op":"rename","steps":[{"k":"mf","path":"/x","id":{"d":1,"i":2,"k":32768,"l":0,"m":[0,0],"c":[0,0],"b":[1]}}]}],"redo":[]}"#),
    ] {
        assert!(crate::backend::undocodec::decode(body).is_none(), "{} record trusted", tag);
    }
}

// A record written before birth time was kept has no "b"; it decodes as unknown and judges by dev, inode and kind.
#[test]
fn a_record_from_before_birth_time_was_kept_decodes_as_unknown() {
    let old = r#"{"v":2,"undo":[{"op":"rename","steps":[{"k":"mf","path":"/x","id":{"d":1,"i":2,"k":32768,"l":0,"m":[0,0],"c":[0,0]}}]}],"redo":[]}"#;
    let doc = crate::backend::undocodec::decode(old).expect("an old record still reads");
    let Step::MadeFile { identity, .. } = &doc.undo[0].steps[0] else { panic!("the step kind survives") };
    assert_eq!(identity.born(), None);
    let born_later = ItemIdentity::from_parts(1, 2, 0o100000, 0, (0, 0), (0, 0), Some((5, 6)));
    assert!(identity.same_item(&born_later), "with no birth time on one side the old check decides");
}

// A doc past 32 MiB trims oldest first and keeps the newest; the render decides, never a count.
#[test]
fn an_oversized_doc_trims_oldest_and_keeps_newest() {
    let big = "op-".to_string() + &"x".repeat(1024 * 1024);
    let mut doc = super::Doc {
        undo: Vec::new(),
        redo: Vec::new(),
        push_gen: 0,
        job_gen: 0,
    };
    for index in 0..40 {
        let path = PathBuf::from(format!("/x{}", index));
        doc.undo.push(Entry { op: format!("{}-{}", big, index), steps: vec![Step::Created { path }] });
    }
    assert!(crate::jsondoc::render(&crate::backend::undocodec::encode(&doc)).len() as u64 > super::MAX_FILE_BYTES);
    let text = super::staged_to_fit(&doc).expect("trimmed doc fits the cap").text();
    assert!(text.len() as u64 <= super::MAX_FILE_BYTES && text.contains(&format!("{}-{}\"", big, 39)), "fits, newest stays");
    assert!(!text.contains(&format!("{}-{}\"", big, 0)), "the oldest record went");
}

// One entry over the cap leaves a barrier in the file; no window keeps the payload.
#[test]
fn a_single_entry_over_the_cap_stores_a_barrier_and_keeps_history() {
    let sandbox = TestDir::new("xundo-toobig");
    let dir = runtime_0700(&sandbox);
    let f1 = sandbox.path().join("f1");
    let f2 = sandbox.path().join("f2");
    let fh = sandbox.path().join("fh");
    std::fs::write(&f1, "1").unwrap();
    std::fs::write(&f2, "2").unwrap();
    std::fs::write(&fh, "h").unwrap();
    let mut a = shared_journal(&sandbox);
    a.push(Entry { op: "small1".to_string(), steps: vec![Step::Created { path: f1.clone() }] });
    a.push(Entry { op: "small2".to_string(), steps: vec![Step::Created { path: f2.clone() }] });
    a.push(huge_entry(fh.clone()));
    let bytes = std::fs::read(dir.join(JOURNAL_FILE)).unwrap();
    assert!(bytes.len() as u64 <= super::MAX_FILE_BYTES, "history preserved under cap");
    let back = crate::backend::undocodec::decode(&String::from_utf8(bytes).unwrap()).unwrap();
    assert_eq!(back.undo.len(), 3, "the barrier keeps the huge entry's place in line");
    let err = a.undo().expect_err("the recorder holds no payload either");
    assert_eq!(err.msg, BARRIER_MSG);
    assert!(fh.exists(), "the barrier reverses nothing");
    let mut b = shared_journal(&sandbox);
    assert_eq!(b.undo().unwrap(), "small2", "older entries still undo from the file");
}

// A foreign window consumes the barrier with the same sentence and keeps nothing for the recorder.
#[test]
fn a_foreign_claim_of_a_huge_entry_consumes_the_barrier() {
    let sandbox = TestDir::new("xundo-foreign");
    let fh = sandbox.path().join("fh");
    std::fs::write(&fh, "h").unwrap();
    let mut a = shared_journal(&sandbox);
    a.push(huge_entry(fh.clone()));
    let mut b = shared_journal(&sandbox);
    let err = b.undo().expect_err("a barrier answers a sentence in any window");
    assert_eq!(err.where_, "undo");
    assert_eq!(err.msg, BARRIER_MSG);
    assert!(fh.exists(), "the barrier reverses nothing");
    assert!(a.undo().is_err(), "the barrier was spent by the foreign claim");
}

// A too-big push clears the shared redo stack through the normal store path.
#[test]
fn a_too_big_push_clears_the_shared_redo_stack() {
    let sandbox = TestDir::new("xundo-redoclear");
    let (from, to, steps) = rename_steps(&sandbox, "a.txt", "b.txt");
    let mut a = shared_journal(&sandbox);
    a.push(Entry { op: "rename".to_string(), steps });
    assert_eq!(a.undo().unwrap(), "rename");
    assert!(from.exists());
    let (tx, _rx) = channel();
    assert_eq!(a.redo(1, &AtomicBool::new(false), &tx).unwrap(), "rename");
    assert!(to.exists());
    assert_eq!(a.undo().unwrap(), "rename");
    let fh = sandbox.path().join("fh");
    std::fs::write(&fh, "h").unwrap();
    a.push(huge_entry(fh));
    let info = a.redo_info().unwrap_err();
    assert_eq!(info.msg, "there is nothing to redo");
}

// A too-big push is a barrier in line, not a payload elsewhere: C, the barrier sentence, then A.
#[test]
fn a_too_big_push_keeps_global_order_through_a_barrier() {
    let sandbox = TestDir::new("xundo-barrier-order");
    let _dir = runtime_0700(&sandbox);
    let fa = sandbox.path().join("fa");
    let fb = sandbox.path().join("fb");
    std::fs::write(&fa, "1").unwrap();
    std::fs::write(&fb, "h").unwrap();
    let mut a = shared_journal(&sandbox);
    a.push(Entry { op: "small-a".to_string(), steps: vec![Step::Created { path: fa.clone() }] });
    a.push(huge_entry(fb.clone()));
    let (rc_from, rc_to, rc_steps) = rename_steps(&sandbox, "rc.txt", "rc-renamed.txt");
    for path in [&rc_from, &rc_to] {
        sandbox.assert_contains(path);
    }
    a.push(Entry { op: "small-c".to_string(), steps: rc_steps });
    // Global order holds across the too-big push, so the newest goes first.
    assert_eq!(a.undo().unwrap(), "small-c");
    assert!(rc_from.exists());
    let err = a.undo().expect_err("a barrier answers a sentence, never a payload");
    assert_eq!(err.where_, "undo");
    assert_eq!(err.msg, BARRIER_MSG);
    assert!(fb.exists(), "the barrier reverses nothing");
    // A barrier never enters redo: the stack below still names small-c, not the barrier.
    assert_eq!(a.redo_info().unwrap(), ("small-c".to_string(), 1));
    assert_eq!(a.undo().unwrap(), "small-a");
    assert!(!fa.exists());
}

// Any window consumes a barrier with one sentence; older entries stay reachable after the recorder exits.
#[test]
fn a_barrier_is_consumed_once_and_blocks_nothing_older() {
    let sandbox = TestDir::new("xundo-barrier-once");
    let _dir = runtime_0700(&sandbox);
    let fa = sandbox.path().join("fa");
    let fh = sandbox.path().join("fh");
    std::fs::write(&fa, "1").unwrap();
    std::fs::write(&fh, "h").unwrap();
    {
        let mut a = shared_journal(&sandbox);
        a.push(Entry { op: "small-a".to_string(), steps: vec![Step::Created { path: fa.clone() }] });
        a.push(huge_entry(fh.clone()));
        // The recorder exits with the payload held nowhere.
    }
    let mut b = shared_journal(&sandbox);
    let err = b.undo().expect_err("a barrier answers a sentence in any window");
    assert_eq!(err.msg, BARRIER_MSG);
    assert!(fh.exists(), "the oversized payload was never reversed");
    let mut c = shared_journal(&sandbox);
    assert_eq!(c.undo().unwrap(), "small-a", "one claim spent the barrier; older entries follow");
    assert!(!fa.exists());
    assert!(c.undo().is_err(), "nothing is left behind the barrier");
}

// A file from a newer writer is unavailable, never rewritten: the push falls back to memory.
#[test]
fn a_newer_version_file_is_unavailable_and_never_rewritten() {
    let sandbox = TestDir::new("xundo-newer");
    let dir = runtime_0700(&sandbox);
    let before = r#"{"v":4,"gen":0,"undo":[],"redo":[]}"#;
    std::fs::write(dir.join(JOURNAL_FILE), before).unwrap();
    let target = sandbox.path().join("f");
    std::fs::write(&target, "1").unwrap();
    let mut journal = shared_journal(&sandbox);
    journal.push(Entry { op: "small".to_string(), steps: vec![Step::Created { path: target.clone() }] });
    let after = std::fs::read_to_string(dir.join(JOURNAL_FILE)).unwrap();
    assert_eq!(after, before, "a newer file is never rewritten by an older reader");
    assert_eq!(journal.undo().unwrap(), "small", "the refused push fell back to memory");
    assert!(!target.exists());
}

// Undo, redo and the redo preview all say one sentence when the shared file is a newer Flea's.
#[test]
fn a_journal_a_newer_flea_wrote_reads_as_history_that_could_not_be_read() {
    let sandbox = TestDir::new("xundo-newer-sentence");
    let dir = runtime_0700(&sandbox);
    std::fs::write(dir.join(JOURNAL_FILE), r#"{"v":4,"gen":0,"undo":[],"redo":[]}"#).unwrap();
    const UNREADABLE: &str = "the undo history could not be read";
    assert_eq!(shared_journal(&sandbox).undo().unwrap_err().msg, UNREADABLE);
    assert_eq!(shared_journal(&sandbox).redo_info().unwrap_err().msg, UNREADABLE);
    let (tx, _rx) = channel();
    assert_eq!(shared_journal(&sandbox).redo(1, &AtomicBool::new(false), &tx).unwrap_err().msg, UNREADABLE);
}

// The codec reads v1 through v3 and refuses anything newer.
#[test]
fn the_codec_reads_v1_v2_and_v3_and_refuses_newer() {
    assert!(crate::backend::undocodec::decode(r#"{"v":1,"gen":0,"undo":[],"redo":[]}"#).is_some(), "v1 still reads");
    assert!(crate::backend::undocodec::decode(r#"{"v":2,"gen":0,"undo":[{"op":"huge","steps":[{"k":"barrier"}]}],"redo":[]}"#).is_some(), "v2 carries the barrier kind");
    assert!(crate::backend::undocodec::decode(r#"{"v":3,"gen":0,"undo":[],"redo":[]}"#).is_some(), "v3 is current");
    assert!(crate::backend::undocodec::decode(r#"{"v":4,"gen":0,"undo":[],"redo":[]}"#).is_none(), "v4 reads as nothing");
}

// Attaching after pushes moves them into the shared doc oldest first; nothing stays local.
#[test]
fn attaching_after_memory_pushes_moves_them_into_the_shared_doc() {
    let sandbox = TestDir::new("xundo-attach-drain");
    let fa = sandbox.path().join("fa");
    let fb = sandbox.path().join("fb");
    std::fs::write(&fa, "1").unwrap();
    std::fs::write(&fb, "2").unwrap();
    let mut journal = Journal::new();
    journal.push(Entry { op: "first".to_string(), steps: vec![Step::Created { path: fa.clone() }] });
    journal.push(Entry { op: "second".to_string(), steps: vec![Step::Created { path: fb.clone() }] });
    assert_eq!(journal.len(), 2);
    journal.attach_test_shared(runtime(&sandbox));
    assert!(journal.is_empty(), "every local entry moved through the normal store path");
    let mut other = shared_journal(&sandbox);
    assert_eq!(other.undo().unwrap(), "second", "oldest first in, newest first out");
    assert!(!fb.exists());
    assert_eq!(other.undo().unwrap(), "first");
    assert!(!fa.exists());
}

// A directory at the journal path loads as empty but never publishes, so the barrier fails.
#[test]
fn a_too_big_push_whose_barrier_is_refused_keeps_the_entry_in_memory() {
    let sandbox = TestDir::new("xundo-barrier-refused");
    let dir = runtime_0700(&sandbox);
    std::fs::create_dir(dir.join(JOURNAL_FILE)).unwrap();
    let target = sandbox.path().join("f");
    std::fs::write(&target, "1").unwrap();
    let mut journal = shared_journal(&sandbox);
    let entry = huge_entry(target.clone());
    let want_len = entry.op.len();
    journal.push(entry);
    let op = journal.undo().unwrap();
    assert_eq!(op.len(), want_len, "the refused barrier kept the full entry in memory");
    assert!(!target.exists(), "the in-memory entry reverses the file it created");
}
#[test]
fn a_push_between_claim_and_finish_drops_the_stale_replay() {
    let sandbox = TestDir::new("xundo-gen");
    let dir = runtime_0700(&sandbox);
    let shared = Shared::at(dir).unwrap();
    let made = |name: &str| Entry { op: name.to_string(), steps: vec![Step::Created { path: PathBuf::from(format!("/{}", name)) }] };
    super::push_entry(&shared, &made("first")).unwrap();
    let (claimed, gen) = super::claim_undo(&shared).unwrap().unwrap();
    assert_eq!(claimed.op, "first");
    super::push_entry(&shared, &made("second")).unwrap();
    super::finish_undone(&shared, Some(claimed), &[], gen).unwrap();
    let info = super::redo_info(&shared).unwrap_err();
    assert_eq!(info.msg, "there is nothing to redo", "stale replay dropped");
    let (top, _) = super::claim_undo(&shared).unwrap().unwrap();
    assert_eq!(top.op, "second", "the pushed entry still undoes");
}

#[path = "undoshare_preserve_tests.rs"]
mod preserve;
