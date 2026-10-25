use super::*;
use crate::backend::events::Event;
use crate::backend::searchreq::Answer;
use crate::backend::searchsession::Session;
use crate::backend::testdir::TestDir;
use std::os::unix::ffi::OsStrExt;
use std::sync::mpsc::Receiver;
use std::time::Duration;

pub(super) const DEADLINE: Duration = Duration::from_secs(10);

// Two Red files (one also Blue), one Yellow, one untagged, and a Red file inside a folder.
pub(super) fn fixture(tag: &str) -> TestDir {
    let d = TestDir::new(tag);
    d.dir("sub");
    for (name, names) in [
        ("red-holiday.txt", vec!["Red"]),
        ("red-notes.txt", vec!["Red", "Blue"]),
        ("yellow-holiday.txt", vec!["Yellow"]),
        ("plain-holiday.txt", vec![]),
        ("sub/red-deep.txt", vec!["Red"]),
    ] {
        let path = d.file(name, "");
        let names: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        crate::backend::tags::write_names(&path, &names).expect("this filesystem keeps user.xdg.tags");
    }
    d
}

// A session with its walk landed and its landing event taken, the way the loop sees an idle one.
pub(super) fn opened(d: &TestDir) -> (Session, Receiver<Event>) {
    let root = d.path().to_str().expect("utf-8").to_string();
    let (tx, rx) = crate::backend::events::channel();
    let mut s = Session::open_async(&root, &root, false, tx);
    s.join_walk();
    crate::backend::searchwalkhold::take_joined_walk(&rx, DEADLINE);
    (s, rx)
}

// Asks a tagged query: the answer stands partial while the names are read, and the worker's landing re-runs it whole.
pub(super) fn tagged(s: &mut Session, rx: &Receiver<Event>, gen: usize, query: &str) -> Answer {
    s.tags.hold_read();
    let partial = s.query(gen, query, 50).expect("a newer generation answers");
    assert!(partial.walking, "{query}: an answer on unread names says it is still walking");
    s.tags.release_read();
    crate::backend::searchwalkhold::wait_landed(rx, DEADLINE);
    assert!(s.poll_walked(), "the loop sees the tag landing");
    let whole = s.re_answer_standing().expect("the standing tagged query is re-run once");
    assert!(!whole.walking, "{query}: the re-run is whole");
    assert_eq!(whole.gen, gen, "the re-run keeps the query's gen");
    whole
}

pub(super) fn names(answer: &Answer) -> Vec<String> {
    let mut rows: Vec<String> = answer.rows.iter().map(|(path, _)| path.clone()).collect();
    rows.sort();
    rows
}

#[test]
fn a_tag_term_keeps_the_entries_that_carry_it() {
    let d = fixture("tagsred");
    let (mut s, rx) = opened(&d);
    let answer = tagged(&mut s, &rx, 1, "tag:Red");
    assert_eq!(answer.total, 3, "red-holiday, red-notes and sub/red-deep carry Red");
    assert_eq!(names(&answer), ["red-holiday.txt", "red-notes.txt", "sub/red-deep.txt"]);
}

#[test]
fn a_tag_term_and_text_both_hold() {
    let d = fixture("tagsredtext");
    let (mut s, rx) = opened(&d);
    let answer = tagged(&mut s, &rx, 1, "tag:Red holiday");
    assert_eq!(names(&answer), ["red-holiday.txt"], "Red and the text holiday leave one entry");
}

#[test]
fn the_tag_name_matches_in_any_case_and_every_named_tag_must_be_present() {
    let d = fixture("tagscase");
    let (mut s, rx) = opened(&d);
    assert_eq!(tagged(&mut s, &rx, 1, "TAG:rEd").total, 3, "the tag name is case-insensitive");
    // The names are in from the first query, so this one answers whole at once.
    let both = s.query(2, "tag:red tag:blue", 50).expect("answers");
    assert!(!both.walking);
    assert_eq!(names(&both), ["red-notes.txt"], "only red-notes carries both");
}

#[test]
fn a_query_with_no_tag_term_reads_no_attribute_and_starts_no_worker() {
    let d = fixture("tagsnone");
    let (mut s, _rx) = opened(&d);
    let answer = s.query(1, "holiday", 50).expect("answers");
    assert!(!answer.walking && answer.total == 3, "a plain query answers whole: {}", answer.total);
    assert_eq!(s.tags.reads(), 0, "no tag term, no getxattr");
    let words = s.query(2, "tag: holiday", 50).expect("answers");
    assert_eq!(s.tags.reads(), 0, "a bare tag: is text, not a term");
    assert!(!words.walking);
}

#[test]
fn each_entry_is_read_once_per_session_and_an_extension_never_reads_again() {
    let d = fixture("tagsonce");
    let (mut s, rx) = opened(&d);
    let first = tagged(&mut s, &rx, 1, "tag:Red");
    let rows = first.indexed;
    assert_eq!(s.tags.reads(), rows, "one read per indexed entry");
    let narrowed = s.query(2, "tag:Red holiday", 50).expect("answers");
    assert!(!narrowed.walking, "the names are in, so the extension answers whole at once");
    assert_eq!(narrowed.total, 1);
    assert_eq!(s.counters.rows_scored, rows, "edit-eligible text rescans candidates while reusing every tag read");
    let other = s.query(3, "tag:Blue", 50).expect("answers");
    assert_eq!(other.total, 1, "another tag is answered from the same names");
    assert_eq!(s.tags.reads(), rows, "no second read for the extension or another tag");
}

#[test]
fn a_tag_extension_does_not_narrow_to_the_shorter_tag_name() {
    let d = fixture("tagsnarrow");
    let (s, _rx) = opened(&d);
    assert!(!s.tags.narrows("tag:R", "tag:Re"), "tag:Re is another term, not the same one grown");
    assert!(!s.tags.narrows("hol", "tag:Red hol"), "a tag term is a new filter");
    assert!(s.tags.narrows("hol", "holi"), "a plain extension narrows as before");
}

#[test]
fn an_empty_text_with_a_tag_matches_every_carrying_entry() {
    let d = fixture("tagsempty");
    let (mut s, rx) = opened(&d);
    let answer = tagged(&mut s, &rx, 1, "tag:yellow");
    assert_eq!(names(&answer), ["yellow-holiday.txt"]);
}

// The names swap in, the landing event arrives, and the standing query is re-run once.
pub(super) fn landed(s: &mut Session, rx: &Receiver<Event>) -> Answer {
    crate::backend::searchwalkhold::wait_landed(rx, DEADLINE);
    assert!(s.poll_walked(), "the loop sees the rewrite landing");
    let whole = s.re_answer_standing().expect("the standing tagged query is re-run once");
    assert!(!whole.walking, "the re-run is whole");
    assert!(!s.poll_walked() && s.re_answer_standing().is_none(), "one landing, one re-run");
    whole
}

#[test]
fn a_patch_keeps_the_finished_pass_and_reads_only_the_rows_it_added() {
    let d = fixture("tagspatch");
    let (mut s, rx) = opened(&d);
    let first = tagged(&mut s, &rx, 1, "tag:Red");
    let before = s.tags.reads();
    let added = d.file("red-new.txt", "");
    crate::backend::tags::write_names(&added, &["Red".to_string()]).expect("tagged");
    assert!(s.patch_after_write(&added), "the patch lands");
    // The standing answer stays whole across the patch: no row drops out and comes back.
    let between = s.query(2, "tag:Red", 50).expect("answers");
    assert!(!between.walking && between.total >= first.total, "whole at once: walking {} total {}", between.walking, between.total);
    let after = landed(&mut s, &rx);
    assert_eq!(after.total, 4, "the added entry is read under the new numbering");
    assert_eq!(s.tags.reads(), before + 1, "no row but the added one was read again");
}

#[test]
fn a_patch_that_drops_rows_moves_the_surviving_names_to_their_new_ids() {
    let d = fixture("tagspatchdrop");
    let (mut s, rx) = opened(&d);
    tagged(&mut s, &rx, 1, "tag:Red");
    let before = s.tags.reads();
    let gone = d.join("red-holiday.txt");
    std::fs::remove_file(&gone).expect("removed");
    assert!(s.patch_after_write(&gone));
    let red = s.query(2, "tag:Red", 50).expect("answers");
    assert!(!red.walking, "the finished pass answers whole after the patch");
    assert_eq!(names(&red), ["red-notes.txt", "sub/red-deep.txt"]);
    assert_eq!(names(&s.query(3, "tag:Yellow", 50).expect("answers")), ["yellow-holiday.txt"], "every later row kept its own names");
    assert_eq!(s.tags.reads(), before, "a removal reads nothing");
}

#[test]
fn a_tag_query_on_a_pass_not_started_keeps_no_row() {
    let (tx, _rx) = crate::backend::events::channel();
    let mut tags = Tags::new(tx);
    let terms = Tags::terms("tag:Red");
    let index = Arc::new(Mutex::new(Index::new()));
    assert!(tags.prepare(&terms, &index, Path::new("/scope"), "", true), "a walk still running leaves the answer partial");
    let mut hits = vec![(10, 0), (9, 1), (8, 2)];
    tags.keep(&mut hits, &terms);
    assert!(hits.is_empty(), "no pass has read a name, so no untagged row is kept");
    let mut plain = vec![(10, 0)];
    tags.keep(&mut plain, &Tags::terms("holiday"));
    assert_eq!(plain, [(10, 0)], "a query with no tag term is left alone");
}


// Seams on a session's tag state: reads made, and a wait for a session opened with no event receiver.
impl Tags {
    // Test seam for a session opened with no event receiver: blocks until the first read has landed its names.
    pub(crate) fn wait_read(&self) {
        let started = std::time::Instant::now();
        while !self.pass.as_ref().is_some_and(|pass| pass.done.load(Ordering::Relaxed)) {
            assert!(started.elapsed() < std::time::Duration::from_secs(10), "the tag read lands");
            std::thread::yield_now();
        }
    }

    // Holds the next pass before its first chunk, so its first answer is partial whatever the fixture size.
    pub(crate) fn hold_read(&mut self) {
        self.hold = true;
    }

    // Opens the gate: the held pass, if started, reads on, and later passes start open.
    pub(crate) fn release_read(&mut self) {
        self.hold = false;
        if let Some(pass) = &self.pass {
            pass.held.store(false, Ordering::Release);
        }
    }

    pub(crate) fn reads(&self) -> usize {
        self.pass.as_ref().map(|pass| pass.reads.load(Ordering::Relaxed)).unwrap_or(0)
    }
}

// holiday.jpg carries Red, notes.txt carries nothing.
fn holiday(tag: &str) -> TestDir {
    let d = TestDir::new(tag);
    d.file("holiday.jpg", "");
    d.file("notes.txt", "");
    crate::backend::tags::write_names(&d.join("holiday.jpg"), &["Red".to_string()]).unwrap();
    d
}

fn rows(s: &mut Session, gen: usize, query: &str) -> Vec<String> {
    let answer = s.query(gen, query, 100).expect("a newer generation answers");
    answer.rows.iter().map(|(path, _)| path.clone()).collect()
}

#[test]
fn a_tag_name_typed_one_key_at_a_time_still_finds_its_files() {
    let d = holiday("tagtyped");
    let (mut s, rx) = opened(&d);
    assert!(names(&tagged(&mut s, &rx, 1, "tag:R")).is_empty(), "tag:R names no tag any file carries");
    assert!(rows(&mut s, 2, "tag:Re").is_empty(), "tag:Re names no tag any file carries");
    assert_eq!(rows(&mut s, 3, "tag:Red"), ["holiday.jpg"], "the whole name finds the file the prefixes missed");
}

#[test]
fn a_tag_prefix_typed_from_the_bare_word_still_finds_its_files() {
    let d = holiday("tagbare");
    let (mut s, rx) = opened(&d);
    for (i, query) in ["t", "ta", "tag", "tag:"].iter().enumerate() {
        rows(&mut s, i + 1, query);
    }
    tagged(&mut s, &rx, 5, "tag:R");
    assert_eq!(rows(&mut s, 6, "tag:Re"), Vec::<String>::new());
    assert_eq!(rows(&mut s, 7, "tag:Red"), ["holiday.jpg"], "typing tag:Red from t finds the Red file");
}

#[test]
fn text_typed_after_a_tag_term_still_narrows_from_the_previous_matches() {
    let d = holiday("tagnarrow");
    let (mut s, rx) = opened(&d);
    assert_eq!(names(&tagged(&mut s, &rx, 1, "tag:Red")), ["holiday.jpg"]);
    assert_eq!(rows(&mut s, 2, "tag:Red hol"), ["holiday.jpg"]);
    assert_eq!(s.counters.rows_scored, 1, "the same tag set scores only the one previous match, not the index");
}

#[test]
fn a_foreign_non_ascii_tag_matches_in_any_case_in_the_live_search() {
    let d = TestDir::new("tagforeign");
    let path = d.file("report.txt", "");
    d.file("other.txt", "");
    crate::backend::tags::write_names(&path, &["\u{c4}rger".to_string()]).unwrap();
    let (mut s, rx) = opened(&d);
    assert_eq!(names(&tagged(&mut s, &rx, 1, "tag:\u{e4}rger")), ["report.txt"], "the live search folds a foreign name like the walker does");
}

#[test]
fn a_name_that_is_not_utf8_reads_its_own_tags() {
    let d = TestDir::new("tagrawname");
    let odd = d.path().join(std::ffi::OsStr::from_bytes(b"caf\xe9.txt"));
    std::fs::write(&odd, "").unwrap();
    crate::backend::tags::write_names(&odd, &["Red".to_string()]).unwrap();
    d.file("plain.txt", "");
    let (mut s, rx) = opened(&d);
    assert_eq!(tagged(&mut s, &rx, 1, "tag:Red").total, 1, "the worker reads the file by its raw name, not its lossy text");
}

// Sample body: "30 1 0:45 / <sandbox> rw - nfs n:/s rw\n", so every path under the sandbox is remote.
fn remote_body(d: &TestDir) -> String {
    format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", d.path().display())
}

fn rels(names: &[&str]) -> Vec<Vec<u8>> {
    names.iter().map(|name| name.as_bytes().to_vec()).collect()
}

#[test]
fn a_local_scope_reads_its_tags_with_no_mount_hop() {
    let d = holiday("taglocal");
    let body = "1 0 8:1 / / rw - ext4 /dev/a rw\n";
    crate::backend::iomount::test_reset();
    let have = Bound::new(d.path(), body).read(&rels(&["holiday.jpg", "notes.txt"]), &Arc::new(AtomicBool::new(false)));
    assert_eq!(have, [Some(vec!["Red".to_string()]), Some(Vec::<String>::new())]);
    assert_eq!(crate::backend::iomount::test_calls(), 0, "a local scope pays no bounded call per row");
}

#[test]
fn a_remote_scope_reads_a_chunk_of_rows_under_one_bounded_call() {
    let d = holiday("tagremote");
    let bound = Bound::new(d.path(), &remote_body(&d));
    crate::backend::iomount::test_reset();
    let have = bound.read(&rels(&["holiday.jpg", "notes.txt"]), &Arc::new(AtomicBool::new(false)));
    assert_eq!(have, [Some(vec!["Red".to_string()]), Some(Vec::<String>::new())], "only the Red file carries its tag");
    assert_eq!(crate::backend::iomount::test_calls(), 1, "two rows ride one mount bound");
}

#[test]
fn a_stuck_remote_scope_reads_no_tag() {
    let d = holiday("tagstuck");
    let bound = Bound::new(d.path(), &remote_body(&d));
    let _stuck = crate::backend::iomount::test_hold_stuck(d.path().to_path_buf());
    let have = bound.read(&rels(&["holiday.jpg", "notes.txt"]), &Arc::new(AtomicBool::new(false)));
    assert!(have.iter().all(Option::is_none), "an unreadable mount leaves its rows unread, never untagged");
}

#[test]
fn a_failed_bound_chunk_is_unread_and_readable_again_later() {
    let d = holiday("tagboundretry");
    let body = remote_body(&d);
    let bound = Bound::new(d.path(), &body);
    let stuck = crate::backend::iomount::test_hold_stuck(d.path().to_path_buf());
    assert!(bound.read(&rels(&["holiday.jpg", "notes.txt"]), &Arc::new(AtomicBool::new(false))).iter().all(Option::is_none), "a failed chunk leaves its rows unread, not untagged");
    drop(stuck);
    assert_eq!(bound.read(&rels(&["holiday.jpg", "notes.txt"]), &Arc::new(AtomicBool::new(false))), [Some(vec!["Red".to_string()]), Some(Vec::<String>::new())], "a later query reads the same rows again");
}

#[test]
fn a_finished_pass_with_unread_rows_answers_partial_and_retries_them() {
    let d = holiday("tagpartial");
    let (tx, rx) = crate::backend::events::channel();
    let mut index = Index::new();
    index.push("holiday.jpg", false);
    index.push("notes.txt", false);
    let index = Arc::new(Mutex::new(index));
    let pass = Pass::new(2, Bound::new(d.path(), ""));
    pass.names.lock().unwrap()[0] = Some(vec!["Red".to_string()]);
    pass.done.store(true, Ordering::Relaxed);
    pass.unread_failed.store(true, Ordering::Relaxed);
    pass.rewrites.lock().unwrap().busy = false;
    let mut tags = Tags::new(tx);
    tags.pass = Some(Arc::clone(&pass));
    let terms = Tags::terms("tag:Red");
    assert!(tags.prepare(&terms, &index, d.path(), "", false), "rows still unread keep the answer partial, never whole");
    crate::backend::searchwalkhold::wait_landed(&rx, DEADLINE);
    assert_eq!(pass.names.lock().unwrap()[1].as_deref(), Some(&[][..]), "the retry read the missing row");
    assert!(!tags.prepare(&terms, &index, d.path(), "", false), "with every row read the answer is whole");
}

// A two-row pass over `holiday` on a remote body, the way the first worker sees it.
fn remote_pass(d: &TestDir) -> (Arc<Pass>, Arc<Mutex<Index>>) {
    let mut index = Index::new();
    index.push("holiday.jpg", false);
    index.push("notes.txt", false);
    (Pass::new(2, Bound::new(d.path(), &remote_body(d))), Arc::new(Mutex::new(index)))
}

#[test]
fn a_failed_first_read_leaves_its_rows_unread_and_raises_the_flag() {
    let d = holiday("tagreadfail");
    let (pass, index) = remote_pass(&d);
    let (tx, _rx) = crate::backend::events::channel();
    let _stuck = crate::backend::iomount::test_hold_stuck(d.path().to_path_buf());
    read_all(&pass, &index, 2, &tx);
    assert!(pass.names.lock().unwrap().iter().all(Option::is_none), "a failed chunk leaves both rows unread, not untagged");
    assert!(pass.unread_failed.load(Ordering::Relaxed), "the failed chunk raises the flag");
}

#[test]
fn a_failed_reread_drops_the_old_names_and_raises_the_flag() {
    let d = holiday("tagrereadfail");
    let (pass, index) = remote_pass(&d);
    let (tx, _rx) = crate::backend::events::channel();
    read_all(&pass, &index, 2, &tx);
    assert!(!pass.unread_failed.load(Ordering::Relaxed) && pass.names.lock().unwrap()[0].is_some(), "the first read answered whole");
    let _stuck = crate::backend::iomount::test_hold_stuck(d.path().to_path_buf());
    reread(&pass, &index, rels(&["holiday.jpg"]));
    let names = pass.names.lock().unwrap();
    assert!(names[0].is_none(), "a slot whose rewrite re-read failed no longer answers on its stale tags");
    assert!(names[1].is_some(), "a row not re-read keeps its names");
    assert!(pass.unread_failed.load(Ordering::Relaxed), "the failed re-read raises the flag");
}

#[test]
fn a_stuck_mount_gets_one_retry_per_query_and_never_a_cycle() {
    let d = holiday("tagstuckcycle");
    let (mut s, rx) = opened(&d);
    s.body = remote_body(&d);
    crate::backend::iomount::mark_stuck(d.path());
    // Every landing the loop would answer, until the channel stays quiet; a cycle never goes quiet.
    let settle = |s: &mut Session| {
        let mut landings = 0;
        while rx.recv_timeout(std::time::Duration::from_millis(300)).is_ok() {
            landings += 1;
            assert!(landings < 20, "a stuck mount keeps announcing: the retry cycles");
            if s.poll_walked() {
                let answer = s.re_answer_standing();
                assert!(!answer.is_some_and(|answer| !answer.walking && s.tags.whole), "unread rows never make an answer whole");
            }
        }
        landings
    };
    assert!(s.query(1, "tag:Red", 50).expect("answers").walking);
    assert_eq!(settle(&mut s), 2, "the first read lands once and its one retry lands once");
    assert_eq!(s.tags.reads(), 2 * s.indexed(), "every row was read once and retried once");
    assert!(s.query(2, "tag:Red", 50).expect("answers").walking, "a still-unread answer stays partial");
    assert_eq!(settle(&mut s), 1, "a new query arms exactly one more retry");
}

#[test]
fn a_spent_unread_retry_settles_the_answer_partial_and_a_new_query_retries() {
    let d = holiday("tagspentretry");
    let (mut s, rx) = opened(&d);
    s.body = remote_body(&d);
    crate::backend::iomount::mark_stuck(d.path());
    // The last re-run any landing owes, until the channel stays quiet.
    let last_answer = |s: &mut Session| {
        let mut last = None;
        while rx.recv_timeout(std::time::Duration::from_millis(300)).is_ok() {
            if s.poll_walked() {
                last = s.re_answer_standing().or(last);
            }
        }
        last.expect("a landing re-ran the standing query")
    };
    assert!(s.query(1, "tag:Red", 50).expect("answers").walking);
    let settled = last_answer(&mut s);
    assert!(!settled.walking && !s.tags.whole, "a spent retry with rows unread settles partial: no read is in flight");
    assert!(s.re_answer_standing().is_none() && !s.poll_walked(), "the settled answer waits for no landing");
    let reads = s.tags.reads();
    assert!(s.query(2, "tag:Red", 50).expect("answers").walking, "a new query re-arms its one retry, which is in flight");
    assert!(!last_answer(&mut s).walking, "the retried read fails again and settles");
    assert_eq!(s.tags.reads(), reads + s.indexed(), "the new query retried every row once");
}

// A local sandbox whose `nas` folder is a nested nfs mount; both files carry Red and the body names the mount.
fn nested_remote(tag: &str) -> (TestDir, String) {
    let d = TestDir::new(tag);
    d.dir("nas");
    for name in ["plain.jpg", "nas/held.jpg"] {
        d.file(name, "");
        crate::backend::tags::write_names(&d.join(name), &["Red".to_string()]).unwrap();
    }
    let body = format!("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / {} rw - nfs n:/s rw\n", d.join("nas").display());
    (d, body)
}

#[test]
fn a_row_under_a_remote_mount_inside_a_local_scope_reads_under_the_bound() {
    let (d, body) = nested_remote("tagnested");
    crate::backend::iomount::test_reset();
    let have = Bound::new(d.path(), &body).read(&rels(&["plain.jpg", "nas/held.jpg"]), &Arc::new(AtomicBool::new(false)));
    assert_eq!(have, [Some(vec!["Red".to_string()]), Some(vec!["Red".to_string()])], "both Red files are read");
    assert_eq!(crate::backend::iomount::test_calls(), 1, "only the row under the nested mount rides the bound");
}

#[test]
fn a_stuck_mount_inside_a_local_scope_drops_only_its_own_rows() {
    let (d, body) = nested_remote("tagnestedstuck");
    let _stuck = crate::backend::iomount::test_hold_stuck(d.join("nas"));
    let have = Bound::new(d.path(), &body).read(&rels(&["plain.jpg", "nas/held.jpg"]), &Arc::new(AtomicBool::new(false)));
    assert_eq!(have, [Some(vec!["Red".to_string()]), None], "the unreadable mount leaves its own row unread and the local file still reads");
}

fn entries_named(count: usize) -> Vec<(OsString, bool)> {
    (0..count).map(|n| (OsString::from(format!("f{n}.txt")), false)).collect()
}

#[test]
fn a_failed_tag_chunk_leaves_the_rest_of_the_directory_untagged() {
    let entries = entries_named(TAG_CHUNK * 2 + 44);
    let mut calls = 0;
    let tagged = tags_in_chunks(&entries, true, |names| {
        calls += 1;
        if calls == 2 { return None; }
        Some(names.iter().map(|_| vec!["Red".to_string()]).collect())
    });
    assert_eq!(calls, 2, "the failed chunk ends the reads, so a third chunk is never asked");
    assert_eq!(tagged.iter().filter(|tags| !tags.is_empty()).count(), TAG_CHUNK, "only the first chunk's entries carry tags");
    assert!(tagged[..TAG_CHUNK].iter().all(|tags| tags == &["Red"]) && tagged[TAG_CHUNK..].iter().all(Vec::is_empty));
}

#[test]
fn a_hidden_name_the_walk_drops_is_never_read_for_tags() {
    let entries = vec![(OsString::from(".git"), true), (OsString::from("a.txt"), false), (OsString::from(".x"), false)];
    let mut seen = Vec::new();
    let _ = tags_in_chunks(&entries, false, |names| {
        seen.extend(names.iter().map(|name| name.to_string_lossy().into_owned()));
        Some(names.iter().map(|_| Vec::new()).collect())
    });
    assert_eq!(seen, ["a.txt"], "a dot name costs no tag read while hidden names are off");
    let mut every = Vec::new();
    let _ = tags_in_chunks(&entries, true, |names| {
        every.extend(names.iter().map(|name| name.to_string_lossy().into_owned()));
        Some(names.iter().map(|_| Vec::new()).collect())
    });
    assert_eq!(every, [".git", "a.txt", ".x"], "with hidden names on, every name is read");
}

#[test]
fn a_directory_past_one_chunk_reads_its_tags_in_two_bounded_calls() {
    let d = TestDir::new("tagchunks");
    let entries = entries_named(TAG_CHUNK + 2);
    for (name, _) in &entries {
        d.file(&name.to_string_lossy(), "");
    }
    crate::backend::tags::write_names(&d.join("f129.txt"), &["Red".to_string()]).unwrap();
    crate::backend::iomount::test_reset();
    let tagged = read_tags(d.path(), &remote_body(&d), &entries, false);
    assert_eq!(crate::backend::iomount::test_calls(), 2, "130 entries cost two chunks, never 130 calls");
    assert_eq!(tagged[TAG_CHUNK + 1], ["Red"], "the tag in the second chunk is read");
}
