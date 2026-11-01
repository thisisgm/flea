use super::tests::{fixture, landed, names, opened, tagged, DEADLINE};
use super::*;
use crate::backend::testdir::TestDir;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

// An own tag write through the real request: the files it changed, as the loop hands them to the session.
fn written(d: &TestDir, ops: &mut crate::backend::opsdispatch::Ops, rel: &str, op: &str, tag: &str) -> Vec<std::path::PathBuf> {
    let (tx, _rx) = crate::backend::events::channel();
    let mut st = crate::backend::state::State::new(crate::backend::dirsizeworker::Worker::new(tx));
    st.base = d.path().to_path_buf();
    st.listing.push(rel, false);
    crate::backend::tagsreq::apply_tags(&mut Vec::new(), ops, &st, vec![0], op, tag)
}

fn ops() -> crate::backend::opsdispatch::Ops {
    let (tx, _rx) = channel();
    crate::backend::opsdispatch::Ops::new(tx)
}

#[test]
fn adding_a_tag_reaches_the_open_tag_search_and_reads_only_the_written_row() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = fixture("tagswriteadd");
    let (mut s, rx) = opened(&d);
    let first = tagged(&mut s, &rx, 1, "tag:Red");
    assert_eq!(first.total, 3);
    let before = s.tags.reads();
    let mut o = ops();
    let files = written(&d, &mut o, "plain-holiday.txt", "add", "Red");
    assert_eq!(files, [d.join("plain-holiday.txt")], "the request answers the file it changed");
    s.note_tags_written(&files);
    // Between the write and the landing the old names stand: whole, and no row fewer.
    let between = s.query(2, "tag:Red", 50).expect("answers");
    assert!(!between.walking && between.total >= first.total, "an answer in between is whole: walking {} total {}", between.walking, between.total);
    let after = landed(&mut s, &rx);
    assert_eq!(names(&after), ["plain-holiday.txt", "red-holiday.txt", "red-notes.txt", "sub/red-deep.txt"]);
    assert_eq!(after.gen, 2, "the re-run keeps the newest query's gen");
    assert_eq!(s.tags.reads(), before + 1, "exactly the written row was read");
}

#[test]
fn removing_a_tag_drops_the_row_from_the_open_tag_search() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = fixture("tagswriteremove");
    let (mut s, rx) = opened(&d);
    tagged(&mut s, &rx, 1, "tag:Red");
    let before = s.tags.reads();
    let mut o = ops();
    s.note_tags_written(&written(&d, &mut o, "sub/red-deep.txt", "remove", "Red"));
    let after = landed(&mut s, &rx);
    assert_eq!(names(&after), ["red-holiday.txt", "red-notes.txt"]);
    assert_eq!(s.tags.reads(), before + 1);
}

#[test]
fn a_tag_write_with_no_tag_term_asked_reads_nothing_and_announces_nothing() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = fixture("tagswritenone");
    let (mut s, rx) = opened(&d);
    s.query(1, "holiday", 50).expect("answers");
    let mut o = ops();
    s.note_tags_written(&written(&d, &mut o, "plain-holiday.txt", "add", "Red"));
    assert_eq!(s.tags.reads(), 0, "no pass is open, so the write reads nothing");
    crate::backend::searchwalkhold::assert_no_landing(&rx, Duration::from_millis(200));
    assert!(!s.poll_walked());
    assert_eq!(tagged(&mut s, &rx, 2, "tag:Red").total, 4, "the first tag term reads the names as they are now");
}

#[test]
fn a_write_outside_the_scope_or_the_index_reads_nothing() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = fixture("tagswriteout");
    let (mut s, rx) = opened(&d);
    tagged(&mut s, &rx, 1, "tag:Red");
    let before = s.tags.reads();
    let mut o = ops();
    let mut files = written(&d, &mut o, "red-holiday.txt", "add", "Blue");
    files.extend([std::path::PathBuf::from("/elsewhere/a.txt"), d.join("not-indexed.txt")]);
    s.note_tags_written(&files);
    let after = landed(&mut s, &rx);
    assert_eq!(after.total, 3);
    assert_eq!(s.tags.reads(), before + 1, "only the indexed, written row was read");
    assert_eq!(names(&s.query(3, "tag:Blue", 50).expect("answers")), ["red-holiday.txt", "red-notes.txt"]);
}

#[test]
fn undo_and_redo_of_a_tag_write_reach_the_open_tag_search_with_no_directory_patch() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = fixture("tagswriteundo");
    let (mut s, rx) = opened(&d);
    tagged(&mut s, &rx, 1, "tag:Red");
    let mut o = ops();
    s.note_tags_written(&written(&d, &mut o, "plain-holiday.txt", "add", "Red"));
    assert_eq!(landed(&mut s, &rx).total, 4);
    let before = s.tags.reads();
    let undone = crate::backend::opsdispatch::do_undo(&mut Vec::new(), &mut o);
    assert!(undone.dirs.is_empty(), "a tag step changes no directory, so nothing patches the index");
    s.note_tags_written(&undone.tags);
    assert_eq!(landed(&mut s, &rx).total, 3, "the undo takes the row back out");
    assert_eq!(s.tags.reads(), before + 1);
    let (tx, _rx) = channel();
    o.journal.redo(1, &std::sync::atomic::AtomicBool::new(false), &tx).expect("redo reapplies");
    let redone = o.journal.take_last_touched();
    assert!(redone.dirs.is_empty());
    s.note_tags_written(&redone.tags);
    assert_eq!(landed(&mut s, &rx).total, 4, "the redo puts it back");
    assert_eq!(s.tags.reads(), before + 2);
}

#[test]
fn a_slow_mount_write_reaches_the_open_tag_search_through_its_late_message() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = fixture("tagswriteslow");
    let (mut s, rx) = opened(&d);
    tagged(&mut s, &rx, 1, "tag:Red");
    let path = d.join("plain-holiday.txt");
    crate::backend::tags::write_names(&path, &["Red".to_string()]).expect("tagged");
    let (dev, ino) = crate::backend::tagsundo::identity(&path).expect("identity");
    let step = crate::backend::undo::Step::Tags { path: path.clone(), before: None, after: Some(crate::backend::tags::join(&["Red".to_string()])), dev, ino, born: None, generation: None };
    let late = crate::backend::opsreq::OpMsg::SetTagsDone { id: 1, result: Ok((1, vec![step], "add", Vec::new())) };
    assert!(crate::backend::searchreq::op_write_paths(&late).is_empty(), "no directory patch");
    s.note_tags_written(&crate::backend::searchreq::op_tag_paths(&late));
    assert_eq!(landed(&mut s, &rx).total, 4);
}

// A narrowing query asked after the names swapped in and before the loop re-runs the standing query still sees the newly tagged row.
#[test]
fn a_narrowing_tag_query_after_a_tag_write_sees_the_row_it_tagged() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = fixture("tagsredseed");
    let (mut s, rx) = opened(&d);
    tagged(&mut s, &rx, 1, "tag:Red");
    let mut o = ops();
    s.note_tags_written(&written(&d, &mut o, "plain-holiday.txt", "add", "Red"));
    crate::backend::searchwalkhold::wait_landed(&rx, DEADLINE);
    let narrowed = s.query(2, "tag:Red h", 50).expect("answers");
    assert!(names(&narrowed).contains(&"plain-holiday.txt".to_string()), "the seed holds the row the write tagged: {:?}", names(&narrowed));
    assert!(s.poll_walked(), "the landing still stands for its re-run");
    assert!(names(&s.re_answer_standing().expect("re-run once")).contains(&"plain-holiday.txt".to_string()));
}

// An earlier landing that reaches the standing query before the re-read's names are in must not spend the re-run the re-read owes.
#[test]
fn an_earlier_landing_does_not_spend_the_re_run_a_pending_re_read_owes() {
    let _guard = crate::backend::tags::SEAM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let d = fixture("tagsredearly");
    let (mut s, rx) = opened(&d);
    tagged(&mut s, &rx, 1, "tag:Red");
    let before = s.tags.reads();
    let pass = Arc::clone(s.tags.pass.as_ref().expect("a pass is open"));
    // The test holds the names, so the re-read finds and reads its row, then waits to swap it in.
    let held = pass.names.lock().unwrap();
    let mut o = ops();
    s.note_tags_written(&written(&d, &mut o, "plain-holiday.txt", "add", "Red"));
    let started = Instant::now();
    while s.tags.reads() <= before {
        assert!(started.elapsed() < DEADLINE, "the re-read reads its row");
        std::thread::yield_now();
    }
    assert!(!s.tags.rewrite_due("tag:Red"), "an earlier landing finds no landed rewrite to spend");
    drop(held);
    let after = landed(&mut s, &rx);
    assert!(names(&after).contains(&"plain-holiday.txt".to_string()), "the re-run holds the newly tagged row");
}

// Frozen mid-read: the first worker holds a stale read of the row when the write lands, and the queue makes it read the row again.
#[test]
fn a_write_during_the_first_read_is_not_lost() {
    let d = fixture("tagswritefirst");
    let (tx, rx) = crate::backend::events::channel();
    let mut index = Index::new();
    let rels = ["red-holiday.txt", "plain-holiday.txt"];
    for rel in rels {
        index.push(rel, false);
    }
    let index = Arc::new(Mutex::new(index));
    let pass = Pass::new(rels.len(), Bound::new(d.path(), ""));
    let mut tags = Tags::new(tx.clone());
    tags.pass = Some(Arc::clone(&pass));
    // The test holds the names, so the worker finishes its reads and waits to store them.
    let held = pass.names.lock().unwrap();
    let worker = {
        let (pass, index, tx) = (Arc::clone(&pass), Arc::clone(&index), tx.clone());
        std::thread::spawn(move || read_all(&pass, &index, rels.len(), &tx))
    };
    let started = Instant::now();
    while pass.reads.load(Ordering::Relaxed) < rels.len() {
        assert!(started.elapsed() < DEADLINE, "the worker reads both rows");
        std::thread::yield_now();
    }
    // Its stale read of plain-holiday.txt (untagged) is in hand; now the file gains Red and the write is announced.
    crate::backend::tags::write_names(&d.join("plain-holiday.txt"), &["Red".to_string()]).expect("tagged");
    tags.refresh(vec![b"plain-holiday.txt".to_vec()], &index);
    drop(held);
    crate::backend::searchwalkhold::wait_landed(&rx, DEADLINE);
    worker.join().unwrap();
    assert!(pass.done.load(Ordering::Relaxed));
    let names = pass.names.lock().unwrap();
    assert_eq!(names[1].as_deref(), Some(&["Red".to_string()][..]), "the stale first read did not win");
    assert_eq!(pass.reads.load(Ordering::Relaxed), 3, "two first reads and one re-read");
    crate::backend::searchwalkhold::assert_no_landing(&rx, Duration::from_millis(200));
}

// Measured by hand, not by the suite: cargo test --release --bin flea -- --ignored --nocapture tag_pass_at_200k.
#[test]
#[ignore]
fn tag_pass_at_200k_entries_prints_its_ms() {
    let d = TestDir::new("tags200k");
    for dir in 0..200 {
        d.dir(&format!("d{dir:03}"));
        for file in 0..1000 {
            let path = d.file(&format!("d{dir:03}/f{file:04}.txt"), "");
            // One file in a hundred is tagged.
            if (dir * 1000 + file) % 100 == 0 {
                crate::backend::tags::write_names(&path, &["Red".to_string()]).expect("tagged");
            }
        }
    }
    let (mut s, rx) = opened(&d);
    let asked = Instant::now();
    let partial = s.query(1, "tag:Red", 50).expect("answers");
    let loop_ms = asked.elapsed().as_secs_f64() * 1000.0;
    assert!(partial.walking);
    crate::backend::searchwalkhold::wait_landed(&rx, DEADLINE);
    let read_ms = asked.elapsed().as_secs_f64() * 1000.0;
    assert!(s.poll_walked());
    let whole = s.re_answer_standing().expect("re-run");
    println!("TAGS200K entries={} tagged={} partial_answer_ms={loop_ms:.1} names_read_ms={read_ms:.1} rerun_ms={:.1}", whole.indexed, whole.total, whole.ms);
    let narrowed = s.query(2, "tag:Red f0100", 50).expect("answers");
    println!("TAGS200K narrowed_total={} narrowed_ms={:.1}", narrowed.total, narrowed.ms);
    let plain = s.query(3, "f0001", 50).expect("answers");
    println!("TAGS200K untagged_total={} untagged_ms={:.1} reads={}", plain.total, plain.ms, s.tags.reads());
}

fn assert_survivor_tag_queue(add: bool) {
    let d = TestDir::new("fb1tagcarry");
    let mut index = Index::new();
    for name in ["a", "b", "c"] {
        d.file(name, "");
        assert!(index.push(name, false));
    }
    let pass = Pass::new(index.len(), Bound::new(d.path(), ""));
    *pass.names.lock().unwrap() = vec![Some(vec!["red".into()]), Some(vec!["blue".into()]), Some(vec!["green".into()])];
    pass.done.store(true, Ordering::Relaxed);
    let (tx, _rx) = crate::backend::events::channel();
    let mut tags = Tags::new(tx);
    tags.pass = Some(Arc::clone(&pass));
    let gone = d.join("a");
    d.assert_contains(&gone);
    std::fs::remove_file(&gone).expect("remove sandbox entry");
    if add { d.file("d", ""); }
    assert!(index.patch_dir(d.path(), Path::new(""), false, ""));
    let queued = tags.carry(&index);
    assert!(Arc::ptr_eq(tags.pass.as_ref().unwrap(), &pass), "the finished pass stays cached");
    let mut expected = vec![(b"b".to_vec(), Some(vec!["blue".to_string()])), (b"c".to_vec(), Some(vec!["green".to_string()]))];
    if add { expected.push((b"d".to_vec(), None)); }
    let names = pass.names.lock().unwrap().clone();
    assert_eq!(names.len(), index.len());
    let mut carried: Vec<_> = (0..index.len()).map(|row| (index.raw(row), names[row].clone())).collect();
    carried.sort();
    assert_eq!(carried, expected, "surviving identities keep their cached tags");
    for (query, raw) in [("tag:blue", b"b"), ("tag:green", b"c")] {
        let mut hits = (0..index.len()).map(|row| (1, row)).collect::<Vec<_>>();
        tags.keep(&mut hits, &Tags::terms(query));
        assert_eq!(hits.len(), 1);
        assert_eq!(index.raw(hits[0].1), raw);
    }
    let expected_queue = if add { vec![b"d".to_vec()] } else { Vec::new() };
    assert_eq!(queued, expected_queue, "only new identities need tag reads");
    assert_eq!(index.last_patch().1, 2, "both kept rows count as survivors");
    assert_eq!(pass.reads.load(Ordering::Relaxed), 0, "carrying the cache performs no tag reads");
}

#[test]
fn search_fb1_pure_deletion_queues_no_tag_reads() {
    assert_survivor_tag_queue(false);
}

#[test]
fn search_fb1_deletion_and_addition_queue_only_the_new_tag_identity() {
    assert_survivor_tag_queue(true);
}

#[test]
fn a_tag_term_lists_a_typo_row_only_when_no_tagged_basename_holds_the_query() {
    // The typo rule reads the rows the tag keeps, so only a tagged real hit hides a tagged typo row.
    for (hit_tagged, listed) in [(false, ["fleet.txt"]), (true, ["flea.txt"])] {
        let d = fixture("tagstypo");
        let flea = d.file("flea.txt", "");
        let fleet = d.file("fleet.txt", "");
        crate::backend::tags::write_names(&fleet, &["Red".to_string()]).expect("tagged");
        if hit_tagged { crate::backend::tags::write_names(&flea, &["Red".to_string()]).expect("tagged"); }
        let (mut s, rx) = opened(&d);
        assert_eq!(names(&s.query(1, "flea", 50).expect("answers")), ["flea.txt"], "a real hit hides the typo row");
        assert_eq!(names(&tagged(&mut s, &rx, 2, "tag:Red flea")), listed, "flea.txt tagged: {hit_tagged}");
    }
}

#[test]
fn a_tagged_narrowing_answers_on_the_worker() {
    let d = fixture("tagsinline");
    let (mut s, rx) = opened(&d);
    tagged(&mut s, &rx, 1, "tag:Red");
    assert!(s.last_typos && !s.answers_small("tag:Red h"), "a tag term keeps the answer off the loop");
    let previous = s.last_matches.len();
    s.query(2, "tag:Red h", 50).expect("answers");
    assert_eq!(s.counters.rows_scored, previous, "answer() narrowed the tagged extension");
}
