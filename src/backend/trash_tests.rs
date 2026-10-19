// The trash wrapper's tests, beside it so trash.rs keeps under the file budget.
use super::*;
use std::os::unix::process::ExitStatusExt;
use std::rc::Rc;

// An exited-0 gio answer with empty streams, the shape a stand-in returns for a call it absorbs.
pub(super) fn ok_output() -> std::process::Output {
    std::process::Output { status: std::process::ExitStatus::from_raw(0), stdout: Vec::new(), stderr: Vec::new() }
}

// Holds this thread's gio stand-in until the test ends, so a failure never leaks it into the next test.
pub(super) struct StandIn;
impl StandIn {
    pub(super) fn hold(stand_in: StandInFn) -> StandIn {
        STAND_IN.with(|slot| *slot.borrow_mut() = Some(stand_in));
        StandIn
    }
}
impl Drop for StandIn {
    fn drop(&mut self) {
        STAND_IN.with(|slot| *slot.borrow_mut() = None);
    }
}

// Issue 88, nixfred: a path the user's listing still names and the filesystem no longer has is a
// failure, so it never becomes a journal step that undo cannot reverse. tests/ops.sh drives the pair.
#[test]
fn paths_that_are_already_gone_are_failures_rather_than_trashed() {
    let gone = vec![PathBuf::from("/nonexistent/flea-88-a.txt"), PathBuf::from("/nonexistent/flea-88-b.txt")];
    let (entries, failed, reason) = trash_checked(&gone, None).expect("a batch of missing paths is not an error");
    assert!(entries.is_empty(), "nothing was trashed, so nothing is journaled");
    assert_eq!(failed, 2, "both are counted as failures rather than as trashed");
    assert!(reason.contains("already gone"), "the reason names the gone paths: {reason}");
}

#[test]
fn a_drag_release_does_not_fail_when_the_receiver_already_moved_the_file() {
    let gone = vec![PathBuf::new(), PathBuf::from("/nonexistent/flea-drag-release.txt")];
    let (entries, failed, reason) = trash_checked(&gone, None).expect("quiet");
    assert!(entries.is_empty());
    assert_eq!(failed, 0);
    assert!(reason.is_empty(), "nothing failed, so no reason rides along");
}

#[test]
fn gio_runs_under_the_named_wait() {
    let argv = gio_argv(&["trash", "--list"]);
    assert_eq!(&argv[..4], ["timeout", "--kill-after=1s", "10s", "gio"], "the bound gio runs under: {argv:?}");
    assert_eq!(&argv[4..], ["trash", "--list"]);
    assert_eq!(failure_reason(&None, false), "the Trash could not be reached, so nothing was moved");
}

#[test]
fn a_large_batch_is_trashed_in_chunks_each_under_the_wait() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let d = crate::backend::testdir::TestDir::new("trash-chunks");
    let mut paths = Vec::new();
    for n in 0..TRASH_CHUNK + 1 {
        paths.push(d.file(&format!("f{n}.txt"), "x"));
    }
    let calls = Rc::new(AtomicUsize::new(0));
    let seen = Rc::clone(&calls);
    let _stand = StandIn::hold(Rc::new(move |args: &[&str]| {
        if args.first() == Some(&"trash") && !args.contains(&"--list") {
            seen.fetch_add(1, Ordering::Relaxed);
        }
        Some(ok_output())
    }));
    trash_checked(&paths, None).expect("a chunked batch is not an error");
    assert_eq!(calls.load(Ordering::Relaxed), 2, "one chunk per bound, not one call for the batch");
}

// 3.g16: after a chunk gio refuses, the later chunks are not sent; their paths stay where they are and count as failed under gio's own line.
#[test]
fn a_refused_chunk_stops_the_batch_and_leaves_the_rest_in_place() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let d = crate::backend::testdir::TestDir::new("trash-chunk-refused");
    let paths: Vec<PathBuf> = (0..TRASH_CHUNK * 2 + 2).map(|n| d.file(&format!("f{n}.txt"), "x")).collect();
    let calls = Rc::new(AtomicUsize::new(0));
    let seen = Rc::clone(&calls);
    let _stand = StandIn::hold(Rc::new(move |args: &[&str]| {
        if args.contains(&"--list") {
            return Some(ok_output());
        }
        if seen.fetch_add(1, Ordering::Relaxed) == 0 {
            for path in &args[2..] {
                std::fs::remove_file(path).unwrap();
            }
            return Some(ok_output());
        }
        Some(std::process::Output { status: std::process::ExitStatus::from_raw(256), stdout: Vec::new(), stderr: b"gio: Error 1: Permission denied\n".to_vec() })
    }));
    let (ok, failed, reason) = trash_checked(&paths, None).expect("a refused chunk is not an error");
    assert_eq!(calls.load(Ordering::Relaxed), 2, "the third chunk is never sent");
    assert_eq!((ok.len(), failed), (TRASH_CHUNK, TRASH_CHUNK + 2));
    assert_eq!(reason, "gio: Error 1: Permission denied");
    assert!(paths[TRASH_CHUNK..].iter().all(|p| p.exists()), "every refused or unsent path is still there");
}

#[test]
fn a_path_still_present_after_success_is_not_already_gone() {
    let d = crate::backend::testdir::TestDir::new("trash-still-there");
    let kept = d.file("kept.txt", "stays");
    let _stand = StandIn::hold(Rc::new(move |_args: &[&str]| Some(ok_output())));
    let (ok, failed, reason) = trash_checked(std::slice::from_ref(&kept), None).expect("a present path is not an error");
    assert!(ok.is_empty());
    assert_eq!(failed, 1);
    assert!(reason.contains("still there"), "a path gio left behind says so: {reason}");
}

#[test]
fn no_uri_is_journaled_from_a_list_that_could_not_be_read_whole() {
    let d = crate::backend::testdir::TestDir::new("trash-partial-list");
    let can = d.dir("can");
    let gone = d.file("gone.txt", "x");
    let moved = gone.clone();
    let lists = Rc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Rc::clone(&lists);
    let _stand = StandIn::hold(Rc::new(move |args: &[&str]| {
        if args.contains(&"--list") {
            // The read after the trash is cut short: junk on stdout with no exit 0 behind it.
            return if seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0 {
                Some(ok_output())
            } else {
                let line = format!("trash:///gone.txt\t{}\n", moved.to_string_lossy());
                Some(std::process::Output { status: std::process::ExitStatus::from_raw(256),
                    stdout: line.into_bytes(), stderr: Vec::new() })
            };
        }
        let target = can.join("gone.txt");
        std::fs::rename(&moved, &target).unwrap();
        Some(ok_output())
    }));
    let (ok, failed, _) = trash_checked(std::slice::from_ref(&gone), None).expect("a cut list is not an error");
    assert_eq!(failed, 0);
    assert_eq!(ok.len(), 1);
    assert!(ok[0].uri.is_empty(), "a list gio did not exit 0 on journals no URI: {}", ok[0].uri);
}

#[test]
fn a_failed_trash_carries_gios_own_last_line() {
    let output = Some(std::process::Output { status: std::process::ExitStatus::from_raw(256),
        stdout: Vec::new(), stderr: b"gio: Error 1: Permission denied\n".to_vec() });
    assert!(failure_reason(&output, false).contains("Permission denied"));
    let timed_out = Some(std::process::Output { status: std::process::ExitStatus::from_raw(124 << 8),
        stdout: Vec::new(), stderr: Vec::new() });
    assert_eq!(failure_reason(&timed_out, false), "The Trash took too long to answer");
    let silent = Some(std::process::Output { status: std::process::ExitStatus::from_raw(256), stdout: Vec::new(), stderr: Vec::new() });
    assert_eq!(failure_reason(&silent, false), "the items could not be moved to the Trash");
    assert_eq!(failure_reason(&silent, true), "the rest could not be moved to the Trash");
}

// A later chunk that never reached gio follows a chunk that moved its items, so the reason must not say nothing moved.
#[test]
fn a_chunk_that_cannot_reach_the_trash_after_one_that_moved_does_not_say_nothing_moved() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let d = crate::backend::testdir::TestDir::new("trash-second-chunk");
    let paths: Vec<PathBuf> = (0..TRASH_CHUNK + 1).map(|n| d.file(&format!("f{n}.txt"), "x")).collect();
    let calls = Rc::new(AtomicUsize::new(0));
    let seen = Rc::clone(&calls);
    let _stand = StandIn::hold(Rc::new(move |args: &[&str]| {
        if args.first() != Some(&"trash") || args.contains(&"--list") { return Some(ok_output()); }
        if seen.fetch_add(1, Ordering::Relaxed) > 0 { return None; }
        for path in &args[2..] { std::fs::remove_file(path).unwrap(); }
        Some(ok_output())
    }));
    let (ok, failed, reason) = trash_checked(&paths, None).expect("a partly moved batch is not an error");
    assert_eq!((ok.len(), failed), (TRASH_CHUNK, 1));
    assert_eq!(reason, "the Trash could not be reached, so the rest were not moved");
    assert!(!reason.contains("nothing"), "{reason}");
}

#[test]
fn a_gio_that_cannot_restore_says_what_could_not_be_done() {
    let entry = Entry { original: PathBuf::from("/a/x"), uri: "trash:///d/x".into(), before: Vec::new(), trashed_at: String::new() };
    let _stand = StandIn::hold(Rc::new(|_args: &[&str]| {
        Some(std::process::Output { status: std::process::ExitStatus::from_raw(256), stdout: Vec::new(), stderr: Vec::new() })
    }));
    assert_eq!(restore(&entry).unwrap_err().msg, "the item could not be restored from the Trash");
    let _stand = StandIn::hold(Rc::new(|_args: &[&str]| None));
    assert_eq!(restore(&entry).unwrap_err().msg, "gio is not installed, so nothing can be restored from the Trash");
}

#[test]
fn a_list_line_splits_on_the_tab_and_keeps_a_path_containing_spaces() {
    let out = "trash:///a.txt\t/home/gm/a.txt\ntrash:///my%20file\t/home/gm/my file\n";
    let got = parse_list(out);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].uri, "trash:///a.txt");
    assert_eq!(got[0].original, PathBuf::from("/home/gm/a.txt"));
    assert_eq!(got[1].original, PathBuf::from("/home/gm/my file"));
}

#[test]
fn a_malformed_list_line_is_skipped_rather_than_panicking() {
    let got = parse_list("no tab here\n\n\tmissing uri\ntrash:///ok\t/x\ntrash:///nopath\t\n");
    assert_eq!(got.len(), 1, "only the one well-formed line survives");
    assert_eq!(got[0].uri, "trash:///ok");
}

#[test]
fn only_an_entry_absent_before_the_call_is_taken_as_ours() {
    let p = PathBuf::from("/home/gm/dup.txt");
    let before = vec![Entry { original: p.clone(), uri: "trash:///dup.txt".to_string(), before: Vec::new(), trashed_at: String::new() }];
    let after = vec![
        Entry { original: p.clone(), uri: "trash:///dup.txt".to_string(), before: Vec::new(), trashed_at: String::new() },
        Entry { original: p.clone(), uri: "trash:///dup.2.txt".to_string(), before: Vec::new(), trashed_at: String::new() },
    ];
    // Both entries name the same original, which is exactly why the URI is captured at trash time.
    let got = newest_entry_for(&before, &after, &p).expect("the new one");
    assert_eq!(got.uri, "trash:///dup.2.txt");
}

#[test]
fn an_entry_with_no_uri_refuses_to_restore_instead_of_running_gio_with_an_empty_argument() {
    let e = Entry::unresolved(PathBuf::from("/x"), Vec::new(), String::new());
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(std::rc::Rc::new(|args: &[&str]| {
        assert!(!args.contains(&"--restore"), "resolution never restores an empty URI: {args:?}");
        None
    })));
    let err = restore(&e).expect_err("must refuse");
    STAND_IN.with(|slot| *slot.borrow_mut() = None);
    assert_eq!(err.where_, "undo");
    assert!(err.msg.contains("cannot be restored"));
}

#[test]
fn an_unresolvable_deferred_step_names_its_time_in_the_error() {
    use std::os::unix::process::ExitStatusExt;
    let e = Entry::unresolved(PathBuf::from("/x"), vec!["trash:///old".to_string()], "2026-10-06T00:00:00".to_string());
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(std::rc::Rc::new(|_: &[&str]| {
        Some(std::process::Output { status: std::process::ExitStatus::from_raw(0), stdout: Vec::new(), stderr: Vec::new() })
    })));
    let err = restore(&e).expect_err("an empty Trash means nothing to resolve");
    STAND_IN.with(|slot| *slot.borrow_mut() = None);
    assert!(err.msg.contains("no longer listed"), "the error says the entry is gone: {}", err.msg);
    assert!(err.msg.contains("2026-10-06 00:00"), "with the time it was trashed in the app's one date form: {}", err.msg);
    assert!(!err.msg.contains("T00:00") && !err.msg.contains("T16:00"), "never the raw DeletionDate stamp: {}", err.msg);
}

#[test]
fn a_list_that_fails_at_undo_says_so_instead_of_calling_the_item_gone() {
    let e = Entry::unresolved(PathBuf::from("/x"), vec!["trash:///old".to_string()], "2026-10-06T00:00:00".to_string());
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(std::rc::Rc::new(|_: &[&str]| None)));
    let err = restore(&e).expect_err("an unreadable Trash cannot resolve");
    STAND_IN.with(|slot| *slot.borrow_mut() = None);
    assert_eq!(err.msg, "the Trash could not be listed, so that item cannot be restored yet");
}

#[test]
fn a_step_with_no_settle_time_refuses_even_when_its_path_is_listed() {
    use std::os::unix::process::ExitStatusExt;
    let e = Entry::unresolved(PathBuf::from("/a/x"), Vec::new(), String::new());
    let listing = b"trash:///x\t/a/x\n".to_vec();
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(std::rc::Rc::new(move |args: &[&str]| {
        assert!(!args.contains(&"--restore"), "a step with no settle time never restores: {args:?}");
        Some(std::process::Output { status: std::process::ExitStatus::from_raw(0), stdout: listing.clone(), stderr: Vec::new() })
    })));
    let err = restore(&e).expect_err("a step with no time must refuse");
    STAND_IN.with(|slot| *slot.borrow_mut() = None);
    assert_eq!(err.msg, "this item was trashed without a Trash entry, so it cannot be restored", "no list ran, so the refusal claims nothing about it");
}

#[test]
fn two_new_entries_for_one_path_refuse_rather_than_guess() {
    use std::os::unix::process::ExitStatusExt;
    let e = Entry::unresolved(PathBuf::from("/a/x"), vec!["trash:///old".to_string()], "2026-10-07T16:00:00".to_string());
    let listing = b"trash:///x.2\t/a/x\ntrash:///x.3\t/a/x\n".to_vec();
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(std::rc::Rc::new(move |args: &[&str]| {
        assert!(!args.contains(&"--restore"), "two candidates are never restored: {args:?}");
        Some(std::process::Output { status: std::process::ExitStatus::from_raw(0), stdout: listing.clone(), stderr: Vec::new() })
    })));
    let err = restore(&e).expect_err("two new entries for one path cannot be told apart");
    STAND_IN.with(|slot| *slot.borrow_mut() = None);
    assert!(err.msg.contains("more than one new item"), "the refusal names the ambiguity, not a missing entry: {}", err.msg);
    assert!(err.msg.contains("2026-10-07 16:00"), "with the time it was trashed in the app's one date form: {}", err.msg);
    assert!(!err.msg.contains("T00:00") && !err.msg.contains("T16:00"), "never the raw DeletionDate stamp: {}", err.msg);
}

#[test]
fn one_new_entry_with_a_settle_time_restores_it() {
    use std::os::unix::process::ExitStatusExt;
    use std::rc::Rc;
    let e = Entry::unresolved(PathBuf::from("/a/x"), vec!["trash:///old".to_string()], "2026-10-07T16:00:00".to_string());
    let listing = b"trash:///old\t/a/x\ntrash:///x.2\t/a/x\n".to_vec();
    let info = b"  trash::deletion-date: 2026-10-07T16:00:00\n".to_vec();
    let restored = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let seen = Rc::clone(&restored);
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(Rc::new(move |args: &[&str]| {
        if args.get(1) == Some(&"--restore") {
            seen.borrow_mut().push(args[2].to_string());
            return Some(std::process::Output { status: std::process::ExitStatus::from_raw(0), stdout: Vec::new(), stderr: Vec::new() });
        }
        if args.first() == Some(&"info") {
            return Some(std::process::Output { status: std::process::ExitStatus::from_raw(0), stdout: info.clone(), stderr: Vec::new() });
        }
        Some(std::process::Output { status: std::process::ExitStatus::from_raw(0), stdout: listing.clone(), stderr: Vec::new() })
    })));
    let result = restore(&e);
    STAND_IN.with(|slot| *slot.borrow_mut() = None);
    assert!(result.is_ok(), "one new entry with a settle time restores: {result:?}");
    assert_eq!(*restored.borrow(), vec!["trash:///x.2".to_string()], "the one new URI is the one restored");
}

#[test]
fn a_settled_step_never_restores_a_later_item_from_the_same_place() {
    use std::os::unix::process::ExitStatusExt;
    let e = Entry::unresolved(PathBuf::from("/a/x"), vec!["trash:///old".to_string()], "2026-10-07T16:00:00".to_string());
    let listing = b"trash:///old\t/a/x\ntrash:///x.2\t/a/x\n".to_vec();
    let later = b"  trash::deletion-date: 2026-10-07T17:00:00\n".to_vec();
    STAND_IN.with(|slot| *slot.borrow_mut() = Some(std::rc::Rc::new(move |args: &[&str]| {
        assert!(!args.contains(&"--restore"), "a later item is never restored: {args:?}");
        let stdout = if args.first() == Some(&"info") { later.clone() } else { listing.clone() };
        Some(std::process::Output { status: std::process::ExitStatus::from_raw(0), stdout, stderr: Vec::new() })
    })));
    let err = restore(&e).expect_err("a later item from the same place must refuse");
    STAND_IN.with(|slot| *slot.borrow_mut() = None);
    assert!(err.msg.contains("no item from that time"), "the refusal names the time mismatch: {}", err.msg);
    assert!(err.msg.contains("2026-10-07 16:00"), "with the time it was trashed in the app's one date form: {}", err.msg);
    assert!(!err.msg.contains("T00:00") && !err.msg.contains("T16:00"), "never the raw DeletionDate stamp: {}", err.msg);
}

#[test]
fn trashing_nothing_runs_no_subprocess_and_reports_nothing() {
    let (ok, failed, reason) = trash(&[]);
    assert!(ok.is_empty());
    assert_eq!(failed, 0);
    assert!(reason.is_empty());
}

#[test]
fn a_refused_trash_reports_gios_reason_for_the_wire() {
    let d = crate::backend::testdir::TestDir::new("trash-refused-reason");
    let kept = d.file("kept.txt", "stays");
    let _stand = StandIn::hold(Rc::new(move |args: &[&str]| {
        if args.first() == Some(&"trash") && !args.contains(&"--list") {
            return Some(std::process::Output { status: std::process::ExitStatus::from_raw(256),
                stdout: Vec::new(), stderr: b"gio: Operation not supported\n".to_vec() });
        }
        Some(ok_output())
    }));
    let (ok, failed, reason) = trash_checked(std::slice::from_ref(&kept), None).expect("a refused batch is not an error");
    assert!(ok.is_empty());
    assert_eq!(failed, 1);
    assert!(reason.contains("not supported"), "gio's own refusal rides the wire: {reason}");
}

#[test]
fn an_unattributed_settle_round_trips_its_path_before_list_and_time() {
    let e = Entry::unresolved(PathBuf::from("/a/j2"), vec!["trash:///old".to_string()], "2026-10-06T00:00:00".to_string());
    let crate::jsondoc::Json::Obj(pairs) = encode_step(&e) else { panic!("a Trashed step encodes as an object") };
    let Some(crate::backend::undo::Step::Trashed(back)) = decode_step(&pairs) else { panic!("and decodes as a Trashed step") };
    assert_eq!((back.original, back.uri, back.before, back.trashed_at), (e.original, e.uri, e.before, e.trashed_at),
        "an unattributed settle's path, before list and time all come back");
}

#[test]
fn an_ordinary_entry_encodes_with_no_resolution_keys() {
    let e = Entry { original: PathBuf::from("/a/j"), uri: "trash:///j".to_string(), before: Vec::new(), trashed_at: String::new() };
    let crate::jsondoc::Json::Obj(pairs) = encode_step(&e) else { panic!("a Trashed step encodes as an object") };
    let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["k", "path", "uri"], "an ordinary entry writes the same record as before");
}

#[test]
fn a_deletion_stamp_reads_as_seconds_since_the_epoch() {
    assert_eq!(stamp_secs("1970-01-02T00:00:00"), Some(SECS_PER_DAY));
    assert_eq!(stamp_secs("1999-12-31T23:59:59"), Some(946_684_799));
    assert_eq!(stamp_secs("2000-03-01T00:00:00"), Some(951_868_800));
    assert_eq!(stamp_secs("2024-02-29T01:02:03"), Some(1_709_168_523));
    assert_eq!(stamp_secs("2026-10-07T16:00:00"), Some(1_791_388_800));
    assert_eq!(stamp_secs("2026-10-07 16:00:00"), None, "a stamp without the T is no stamp");
}

// Counts gio spawns and their order, never wall time: the announce must come between the trash and the settle list, once per batch.
#[test]
fn the_outcome_is_announced_after_the_trash_and_before_the_settle_list_with_no_spawn_per_item() {
    let d = crate::backend::testdir::TestDir::new("trash-announce-order");
    let can = d.dir("can");
    let items: Vec<PathBuf> = (0..5).map(|n| d.file(&format!("f{n}.txt"), "x")).collect();
    let events = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let (seen, moved) = (Rc::clone(&events), items.clone());
    let _stand = StandIn::hold(Rc::new(move |args: &[&str]| {
        if args.contains(&"--list") {
            seen.borrow_mut().push("list".to_string());
        } else {
            seen.borrow_mut().push("trash".to_string());
            for p in &moved {
                std::fs::rename(p, can.join(p.file_name().unwrap())).unwrap();
            }
        }
        Some(ok_output())
    }));
    let log = Rc::clone(&events);
    let (ok, failed, reason) = trash_announcing(&items, None, &mut |n, f, r| {
        log.borrow_mut().push(format!("announce {} {f} {r:?}", n.len()));
    }).expect("a batch is not an error");
    assert_eq!((ok.len(), failed, reason.as_str()), (5, 0, ""));
    assert_eq!(*events.borrow(), ["list", "trash", "announce 5 0 \"\"", "list"], "one spawn per stage for five items, announce before the settle list");
}

#[test]
fn an_announced_failure_words_itself_as_the_final_result_does() {
    let d = crate::backend::testdir::TestDir::new("trash-announce-failure");
    let kept = d.file("kept.txt", "stays");
    let _stand = StandIn::hold(Rc::new(move |_args: &[&str]| Some(ok_output())));
    let said = Rc::new(std::cell::RefCell::new(None::<(usize, usize, String)>));
    let log = Rc::clone(&said);
    let (entries, failed, reason) = trash_announcing(std::slice::from_ref(&kept), None, &mut |n, f, r| {
        *log.borrow_mut() = Some((n.len(), f, r.to_string()));
    }).expect("a present path is not an error");
    assert!(entries.is_empty());
    assert_eq!(said.borrow().clone(), Some((0, failed, reason)), "the announce and the result agree");
}

// timeout's own exit is 124 on the first signal; after the kill-after it dies with its group, which std reports as signal 9 and no code.
#[test]
fn a_gio_that_outlives_the_first_signal_still_reads_as_taking_too_long() {
    let said = |raw: i32| failure_reason(&Some(std::process::Output { status: std::process::ExitStatus::from_raw(raw), stdout: Vec::new(), stderr: Vec::new() }), false);
    assert_eq!(said(124 << 8), "The Trash took too long to answer", "exit 124");
    assert_eq!(said(9), "The Trash took too long to answer", "killed by signal 9 after the kill-after");
    assert_eq!(said(1 << 8), "the items could not be moved to the Trash", "an ordinary failure with no stderr keeps its own words");
}
