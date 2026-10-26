use super::*;
use crate::backend::searchwalkhold::wait_walk_landed;
use crate::backend::testdir::TestDir;
use std::time::Duration;

// One session, ten queries typed character by character on nested directories.
fn fixture() -> TestDir {
    let d = TestDir::new("sessionten");
    d.dir("docs/work");
    d.dir("downloads");
    d.file("docs/work/bench-notes.txt", "");
    d.file("docs/bench.txt", "");
    d.file("downloads/helper.txt", "");
    d.file("bench.txt", "");
    d
}

// The edit budget rises at the 4th and 8th letter; the hyphen is no letter, so prefixes 4 and 9 rescore the index.
const RESCORED_AT: [usize; 2] = [4, 9];

#[test]
fn ten_queries_cost_one_walk_and_one_mountinfo_read() {
    crate::backend::mountinfo::test_reset();
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    assert_eq!(s.counters.walks_started, 1, "1 walk over the session's directories");
    assert_eq!(s.walk_dirs(), 4, "the scope, docs, docs/work and downloads over 4 directories walked");
    assert_eq!(s.partial_reason(), None, "the fixture has complete coverage");
    assert_eq!(crate::backend::mountinfo::test_reads(), 1, "1 mountinfo read over the session's directories");
    let mut previous = usize::MAX;
    for (i, len) in (1..=10).enumerate() {
        let query: String = "bench-notes".chars().take(len).collect();
        let answer = s.query(i + 1, &query, 100).expect("each newer generation answers");
        assert_eq!(s.counters.walks_started, 1, "still 1 walk after query {}", i + 1);
        assert_eq!(crate::backend::mountinfo::test_reads(), 1, "still 1 mountinfo read after query {}", i + 1);
        if i > 0 {
            let expected = if RESCORED_AT.contains(&len) { s.indexed() } else { previous };
            assert_eq!(s.counters.rows_scored, expected, "safe extensions narrow; budget increases rescore the index");
        }
        previous = answer.total;
    }
}

#[test]
fn an_answer_materializes_only_its_first_window() {
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    // Three bench matches over the fixture, asked with a window of two.
    let answer = s.query(1, "bench", 2).expect("the first query answers");
    assert_eq!(answer.total, 3, "all three matches counted");
    assert_eq!(answer.rows.len(), 2, "only the first window materializes on the key path");
}

#[test]
fn a_scroll_past_the_head_extends_in_chunks_of_the_full_ranked_order() {
    let d = TestDir::new("sessionchunked");
    for i in 0..800 {
        d.file(&format!("bench-{:04}.txt", i), "");
    }
    d.file("other.txt", "");
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    // Eight hundred bench matches over the fixture, asked with a window of two.
    let answer = s.query(1, "bench", 2).expect("the first query answers");
    assert_eq!(answer.total, 800, "all eight hundred matches counted");
    let mut names: Vec<String> = answer.rows.iter().map(|(path, _)| path.clone()).collect();
    assert_eq!(s.pending_rest_len(), 798, "the rest waits unranked for a scroll");
    // The oracle is today's walk path over the same tree and query.
    let mut walk = crate::backend::search::Search::new(&root, "bench", false);
    let mut listing = crate::backend::listing::Listing::new();
    while !walk.step(&mut listing) {}
    walk.rank(&mut listing);
    let walk_names: Vec<String> = (0..listing.len()).map(|i| listing.name(i).to_string()).collect();
    assert_eq!(names, walk_names[..2], "the head is the ranked order's own prefix");
    // Past the head: the request's need is two rows, so one window of 350 joins.
    let chunk = s.take_rest_chunk(4, names.len()).expect("the first chunk extends on demand");
    assert_eq!(chunk.len(), 350, "need or one window, whichever is larger");
    names.extend(chunk.iter().map(|(path, _)| path.clone()));
    assert_eq!(names, walk_names[..352], "head plus chunk one is the ranked order's own prefix");
    assert_eq!(s.pending_rest_len(), 448, "the remainder waits for the next scroll");
    // Past chunk one: 352 adopted, so the chunk doubles to 352 more.
    let chunk = s.take_rest_chunk(452, names.len()).expect("the second chunk extends on demand");
    assert_eq!(chunk.len(), 352, "at least double the adopted length");
    names.extend(chunk.iter().map(|(path, _)| path.clone()));
    assert_eq!(names, walk_names[..704], "two chunks are still the ranked order's own prefix");
    assert_eq!(s.pending_rest_len(), 96, "the tail waits for the last scroll");
    // To the end: the last chunk completes the whole answer in the same order.
    let chunk = s.take_rest_chunk(usize::MAX, names.len()).expect("the tail extends on demand");
    assert_eq!(chunk.len(), 96, "the held remainder");
    names.extend(chunk.iter().map(|(path, _)| path.clone()));
    assert_eq!(names, walk_names, "three scrolls read the whole answer in order");
    assert_eq!(s.pending_rest_len(), 0, "the last chunk spends the held rest");
    assert!(s.take_rest_chunk(usize::MAX, names.len()).is_none(), "a fourth extension answers nothing");
}

// 1,000 matches over ten folders: one score, a hundred path lengths, ten paths tied on each length.
fn wide_fixture() -> TestDir {
    let d = TestDir::new("sessionwide");
    for i in 0..1000 {
        d.dir(&format!("d{}", i / 100));
        d.file(&format!("d{}/bench{}.txt", i / 100, "a".repeat(i % 100)), "");
    }
    d
}

#[test]
fn a_window_ranks_its_head_with_path_compares_only_at_its_edge() {
    let d = wide_fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut walk = crate::backend::search::Search::new(&root, "bench", false);
    let mut listing = crate::backend::listing::Listing::new();
    while !walk.step(&mut listing) {}
    walk.rank(&mut listing);
    let walk_names: Vec<String> = (0..listing.len()).map(|i| listing.name(i).to_string()).collect();
    let mut s = Session::open_sync(&root, &root, false);
    // A window of 45 cuts the ten paths tied on the fifth length, so five of them are ordered by name.
    for (gen, window) in [(1, 45), (2, 50), (3, 1000)] {
        let answer = s.query(gen, "bench", window).expect("each newer generation answers");
        assert_eq!(answer.total, 1000, "1000 matches over 10 folders");
        let names: Vec<String> = answer.rows.iter().map(|(path, _)| path.clone()).collect();
        assert_eq!(names, walk_names[..window], "a window of {} is the ranked order's own prefix", window);
        // The window's own sort costs about window * log2(window) compares; one compare per match is 1000.
        let bound = 2 * window * (usize::BITS - window.leading_zeros()) as usize + 10;
        if window < 1000 {
            assert!(s.counters.rank_compares <= bound,
                "{} path compares for a window of {} over 1000 matches, bound {}", s.counters.rank_compares, window, bound);
        }
    }
}

#[test]
fn backspace_to_a_shorter_query_scores_the_index_with_no_walk() {
    crate::backend::mountinfo::test_reset();
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    let indexed = s.indexed();
    assert!(indexed > 0, "the walk indexed {} rows", indexed);
    let _ = s.query(1, "bench-notes", 100).expect("the long query answers");
    s.query(2, "bench", 100).expect("the shorter query answers");
    assert_eq!(s.counters.walks_started, 1, "1 walk over both queries");
    assert_eq!(s.counters.rows_scored, indexed, "the shorter query scores the {} index rows", indexed);
}

#[test]
fn a_stale_generation_is_never_answered_after_a_newer_one() {
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    let _ = s.query(3, "bench", 100).expect("gen 3 answers");
    assert!(s.query(2, "benc", 100).is_none(), "gen 2 is stale after gen 3 and answers nothing");
    assert!(s.query(3, "bench", 100).is_none(), "gen 3 itself answers only once");
    assert!(s.query(4, "b", 100).is_some(), "gen 4 still answers");
}

#[test]
fn the_index_path_ranks_like_todays_walk_path() {
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    let answer = s.query(1, "bench", 100).expect("the index path answers");
    let session_names: Vec<String> = answer.rows.iter().map(|(path, _)| path.clone()).collect();
    let mut walk = crate::backend::search::Search::new(&root, "bench", false);
    let mut listing = crate::backend::listing::Listing::new();
    while !walk.step(&mut listing) {}
    walk.rank(&mut listing);
    let walk_names: Vec<String> = (0..listing.len()).map(|i| listing.name(i).to_string()).collect();
    assert_eq!(session_names, walk_names, "one tree answers in one order over {} rows", walk_names.len());
}

#[test]
fn an_own_write_two_levels_down_shows_in_the_next_query_with_no_walk() {
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    let before = s.query(1, "bench", 100).expect("the first query answers");
    assert!(!before.rows.is_empty(), "the first query matches over {} rows", before.indexed);
    let added = d.file("docs/work/bench-new.txt", "");
    assert!(s.patch_after_write(&added), "the own write under the scope patches the index");
    let after = s.query(2, "bench-new", 100).expect("the next query answers");
    let names: Vec<String> = after.rows.iter().map(|(path, _)| path.clone()).collect();
    assert!(names.iter().any(|name| name == "docs/work/bench-new.txt"), "the own write shows over {} matches", names.len());
    assert_eq!(s.counters.walks_started, 1, "still 1 walk after the own write");
}

#[test]
fn an_outside_change_two_levels_down_is_visible_without_reopening() {
    crate::backend::mountinfo::test_reset();
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    d.file("docs/work/bench-deep.txt", "");
    assert!(s.apply_watch_deltas(), "the recursive event applies without another query");
    let after = s.query(1, "bench-deep", 100).expect("the query answers");
    assert_eq!(after.total, 1);
    assert_eq!(s.counters.walks_started, 1, "the existing blocks were patched");
    assert_eq!(crate::backend::mountinfo::test_reads(), 1, "no scope reopen or mount-table sweep");
}

#[test]
fn close_frees_the_arena_to_zero_bytes() {
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let s = Session::open_sync(&root, &root, false);
    let bytes = s.index_bytes();
    let indexed = s.indexed();
    assert!(bytes > 0, "the arena holds {} bytes over {} rows", bytes, indexed);
    eprintln!("session index: {} bytes over {} rows, {} bytes per entry", bytes, indexed, bytes / indexed);
    let arena = Arc::downgrade(&s.index);
    let freed = s.close();
    assert_eq!(freed, bytes, "close frees the {} arena bytes it held", bytes);
    assert!(arena.upgrade().is_none(), "no owner holds the arena after close");
}

#[test]
fn a_query_while_walking_re_runs_with_its_own_gen_at_landing() {
    let d = TestDir::new("landingwalk");
    for i in 0..10 {
        d.dir(&format!("d{}", i));
    }
    let root = d.path().to_str().expect("utf-8").to_string();
    let (walked_tx, walked_rx) = crate::backend::events::channel();
    // Park the walk at its first directory, so the query below answers from a partial index.
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(1);
    let mut s = Session::open_async_with_probes(&root, &root, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(std::time::Duration::from_secs(30)).expect("the walk parks at its first directory");
    let partial = s.query(1, "zzz-nomatch", 100).expect("gen 1 answers while walking");
    assert!(partial.walking, "the answer says walking over {} indexed rows", partial.indexed);
    assert_eq!(partial.total, 0, "no match yet over the partial index");
    let _ = release.send(());
    wait_walk_landed(&walked_rx, &s, Duration::from_secs(30), |_| false);
    assert!(s.poll_walked(), "the landing announces once");
    assert!(!s.poll_walked(), "the landing never announces twice");
    let landed = s.re_answer_standing().expect("the standing query re-runs at landing");
    assert_eq!(landed.gen, 1, "the landing carries the standing query's own gen");
    assert!(!landed.walking, "the landing says walking false");
    assert!(landed.ms > 0.0, "the landing carries the walk's ms");
    assert!(s.landing().is_none(), "no second landing re-run");
}

#[test]
fn close_mid_walk_opens_no_directory_after_close() {
    let d = TestDir::new("cancelwalk");
    for i in 0..20 {
        d.dir(&format!("d{}", i));
    }
    let root = d.path().to_str().expect("utf-8").to_string();
    // Single-threaded remote-shaped walk, so the hold lands deterministically mid-walk.
    let body = crate::backend::mountinfo::read_body();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(3);
    let scope = PathBuf::from(&root);
    let thread_probes = Arc::clone(&probes);
    let worker = std::thread::spawn(move || crate::backend::searchwalk::walk(&scope, false, true, &body, &thread_probes));
    parked.recv_timeout(std::time::Duration::from_secs(30)).expect("the walk parks after 3 opens");
    // Close lands mid-walk: the flag goes up while the walker is parked.
    probes.cancel.store(true, Ordering::Relaxed);
    let opened_at_close = probes.opened.load(Ordering::Relaxed);
    let _ = release.send(());
    worker.join().expect("the walk ends after release");
    assert_eq!(
        probes.opened.load(Ordering::Relaxed),
        opened_at_close,
        "0 directories open after close over {} opened",
        opened_at_close
    );
}

#[test]
fn a_remote_shaped_scope_walks_through_the_bound() {
    crate::backend::iomount::test_reset();
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let body = crate::backend::mountinfo::read_body();
    let probes = Arc::new(WalkProbes::new());
    let scope = PathBuf::from(&root);
    let (index, dirs) = crate::backend::searchwalk::walk(&scope, false, true, &body, &probes);
    assert_eq!(dirs, 4, "4 directories walked over the fixture");
    assert!(index.len() >= 6, "the remote walk indexes {} rows", index.len());
    assert!(crate::backend::iomount::test_calls() > 0, "the remote walk takes the bound");
}

#[test]
fn an_update_push_is_due_only_past_the_batch_window() {
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    let _ = s.query(1, "bench", 100).expect("the first query answers");
    assert!(!s.batch_due(), "no update push right after an answer");
    s.test_set_last_batch(Instant::now() - std::time::Duration::from_millis(BATCH_MS));
    assert!(s.batch_due(), "an update push is due past the batch window");
}

#[test]
fn cancel_walk_keeps_the_session_and_its_partial_index() {
    let d = TestDir::new("cancelwalk");
    for i in 0..10 {
        d.dir(&format!("d{}", i));
    }
    d.file("bench.txt", "");
    let root = d.path().to_str().expect("utf-8").to_string();
    let (walked_tx, _walked_rx) = crate::backend::events::channel();
    // Park the walk at its first directory, so the cancel below lands mid-walk.
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(1);
    let mut s = Session::open_async_with_probes(&root, &root, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(std::time::Duration::from_secs(30)).expect("the walk parks at its first directory");
    let partial = s.query(1, "bench", 100).expect("gen 1 answers while walking");
    assert!(partial.walking, "the answer says walking over {} indexed rows", partial.indexed);
    let cancelled = s.cancel_walk().expect("a running walk cancels");
    assert_eq!(cancelled.gen, 1, "the cancel keeps the standing query's gen");
    let answer = cancelled.answer.expect("a standing query re-answers");
    assert_eq!(answer.gen, 1, "the re-answer carries its own gen");
    assert!(!answer.walking, "the re-answer says walking false");
    assert_eq!(cancelled.indexed, s.indexed(), "the kept index size is reported");
    // The landing event still arrives; the cancel swallowed it, so nothing re-runs.
    let _ = release.send(());
    assert!(s.landing().is_none(), "no landing re-run after a cancel");
    let kept = s.query(2, "bench", 100).expect("a later query answers from the kept index");
    assert_eq!(kept.total, answer.total, "the kept index answers alike");
    assert_eq!(s.counters.walks_started, 1, "still 1 walk after cancel and query");
}

#[test]
fn cancel_walk_with_no_walk_running_answers_nothing() {
    let d = fixture();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    assert!(s.cancel_walk().is_none(), "a landed walk has nothing to stop");
    let _ = s.query(1, "bench", 100).expect("gen 1 answers");
    assert!(s.cancel_walk().is_none(), "still nothing to stop after landing");
}

#[test]
fn a_tag_term_filters_the_session_answer() {
    let d = TestDir::new("sessiontag");
    d.file("holiday.jpg", "");
    d.file("notes.txt", "");
    crate::backend::tags::write_names(&d.join("holiday.jpg"), &["Red".to_string()]).unwrap();
    let root = d.path().to_str().expect("utf-8").to_string();
    let mut s = Session::open_sync(&root, &root, false);
    // The first tag term starts the read off the loop and answers partial; held, so a fast read cannot land before the answer.
    s.tags.hold_read();
    assert!(s.query(1, "tag:Red", 100).expect("the tag query answers").walking, "the first ask stands on unread names");
    s.tags.release_read();
    s.tags.wait_read();
    let answer = s.query(2, "tag:Red", 100).expect("the tag query answers");
    let names: Vec<String> = answer.rows.iter().map(|(path, _)| path.clone()).collect();
    assert_eq!(names, ["holiday.jpg"], "only the tagged file answers the tag term");
    let combined = s.query(3, "tag:Red holiday", 100).expect("the tag term combines with the fuzzy text");
    let names: Vec<String> = combined.rows.iter().map(|(path, _)| path.clone()).collect();
    assert_eq!(names, ["holiday.jpg"]);
    let missed = s.query(4, "tag:Red notes", 100).expect("the tag term refuses what the text alone would keep");
    assert_eq!(missed.total, 0, "notes.txt carries no Red tag over {} indexed rows", missed.indexed);
    let plain = s.query(5, "notes", 100).expect("a query with no tag term answers as today");
    let names: Vec<String> = plain.rows.iter().map(|(path, _)| path.clone()).collect();
    assert_eq!(names, ["notes.txt"]);
}

#[test]
fn an_empty_standing_query_lands_one_searched_line() {
    let d = TestDir::new("landingempty");
    for i in 0..6 {
        d.dir(&format!("d{}", i));
    }
    let root = d.path().to_str().expect("utf-8").to_string();
    let (walked_tx, walked_rx) = crate::backend::events::channel();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(1);
    let mut s = Session::open_async_with_probes(&root, &root, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(std::time::Duration::from_secs(30)).expect("the walk parks");
    s.note_folder_count(7);
    s.claim_gen(1, "", 100);
    let _ = release.send(());
    wait_walk_landed(&walked_rx, &s, Duration::from_secs(30), |_| false);
    assert!(s.poll_walked(), "the landing announces once");
    let (gen, n, scanned, _) = s.landing_empty().expect("the empty query lands one line");
    assert_eq!((gen, n, scanned), (1, 7, 7), "the standing gen with the remembered folder count");
    assert!(s.landing_empty().is_none(), "no second landing line");
}
