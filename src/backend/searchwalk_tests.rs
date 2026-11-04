use super::*;
use crate::backend::searchwalkhold::wait_walk_landed;

#[test]
fn search_fix4_landing_wait_consumes_the_final_wake_after_queued_progress() {
    let root = TestDir::new("walk-queued-progress");
    root.file("alpha.txt", "a");
    let (tx, rx) = crate::backend::events::channel();
    tx.send(crate::backend::events::Event::SearchWalked).unwrap();
    let scope = root.path().to_str().unwrap();
    let session = Session::open_async(scope, scope, false, tx);
    let deadline = Instant::now() + Duration::from_secs(30);
    while !session.handle.as_ref().unwrap().is_finished() {
        assert!(Instant::now() < deadline, "the walk finishes");
        std::thread::sleep(Duration::from_millis(1));
    }
    wait_walk_landed(&rx, &session, Duration::from_secs(30), |_| false);
    assert!(rx.try_recv().is_err(), "the real landing must not remain behind queued progress");
    session.close();
}
use crate::backend::testdir::TestDir;
use std::time::Duration;

// A sandbox TestDir that removes itself, with the bench files first so directory order differs from path order.
fn fixture(tag: &str) -> TestDir {
    let d = TestDir::new(tag);
    for i in 0..30 {
        d.file(&format!("bench-{:02}.txt", i), "");
    }
    for i in 0..10 {
        d.dir(&format!("d{}", i));
    }
    for i in 0..30 {
        d.file(&format!("other-{:02}.txt", i), "");
    }
    d
}

// The fuzzy matches of "bench-1": bench-10 to bench-19 plus bench-01 and bench-21, in path order.
fn bench_one_matches() -> Vec<String> {
    let mut names: Vec<String> = (10..20).chain([1, 21]).map(|i| format!("bench-{:02}.txt", i)).collect();
    names.sort();
    names
}

#[test]
fn a_cancelled_walk_narrows_from_every_match_not_the_window() {
    let d = fixture("cancelnarrow");
    let scope = d.path().to_str().expect("utf-8").to_string();
    let (walked_tx, _walked_rx) = crate::backend::events::channel();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(2);
    let mut s = Session::open_async_with_probes(&scope, &scope, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(30)).expect("the walk parks on its second directory");
    let first = s.query(1, "bench", 2).expect("generation 1 answers");
    assert_eq!(first.total, 30, "30 bench rows match over the root's 70 rows");
    s.cancel_walk().expect("a running walk cancels");
    let narrowed = s.query(2, "bench-1", 100).expect("generation 2 answers");
    assert_eq!(narrowed.total, bench_one_matches().len(), "every bench-1 match after cancel, window of 2 or not");
    drop(release);
}

#[test]
fn a_cancelled_walk_that_lands_keeps_every_row_id() {
    let d = fixture("cancelids");
    let scope = d.path().to_str().expect("utf-8").to_string();
    let (walked_tx, walked_rx) = crate::backend::events::channel();
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(2);
    let mut s = Session::open_async_with_probes(&scope, &scope, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(30)).expect("the walk parks on its second directory");
    s.query(1, "bench", 100).expect("generation 1 answers");
    s.cancel_walk().expect("a running walk cancels");
    release.send(()).expect("the parked walk takes its release");
    wait_walk_landed(&walked_rx, &s, Duration::from_secs(30), |_| false);
    let narrowed = s.query(2, "bench-1", 100).expect("generation 2 answers");
    let mut names: Vec<String> = narrowed.rows.iter().map(|(path, _)| path.clone()).collect();
    names.sort();
    assert_eq!(names, bench_one_matches(), "the held matches still name the same rows after the walk lands");
}

#[test]
fn a_parked_walk_thread_leaves_the_queue_to_the_others() {
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(WALK_THREADS);
    if threads < 2 {
        return;
    }
    let d = TestDir::new("walkqueue");
    for i in 0..10 {
        d.dir(&format!("d{}", i));
    }
    let scope = d.path().to_str().expect("utf-8").to_string();
    let (walked_tx, _walked_rx) = crate::backend::events::channel();
    let probes = Arc::new(WalkProbes::new());
    let (parked_root, release_root) = probes.test_hold_walk_at(1);
    let mut s = Session::open_async_with_probes(&scope, &scope, false, walked_tx, Arc::clone(&probes));
    parked_root.recv_timeout(Duration::from_secs(30)).expect("the walk parks on the scope");
    // Every other walk thread has met the empty queue by now.
    std::thread::sleep(Duration::from_millis(200));
    let (parked, release) = probes.test_hold_walk_at(2);
    release_root.send(()).expect("the scope read takes its release");
    parked.recv_timeout(Duration::from_secs(30)).expect("one thread parks on its first subdirectory");
    let mut opened = s.probes_opened();
    for _ in 0..1000 {
        if opened == 11 {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
        opened = s.probes_opened();
    }
    assert_eq!(opened, 11, "the scope and all 10 subdirectories open while one thread is parked");
    release.send(()).expect("the parked thread takes its release");
    s.join_walk();
    assert_eq!(s.walk_dirs(), 11, "11 directories walked");
}

// Close detaches a running walk, which still holds the arena, so nothing is freed until that thread exits.
#[test]
fn close_mid_walk_frees_nothing_until_the_detached_walk_exits() {
    let d = fixture("closewalk");
    let scope = d.path().to_str().expect("utf-8").to_string();
    let (walked_tx, _walked_rx) = crate::backend::events::channel();
    // Parked at its second directory, the walk has the root's rows in the arena when the close lands.
    let probes = Arc::new(WalkProbes::new());
    let (parked, release) = probes.test_hold_walk_at(2);
    let s = Session::open_async_with_probes(&scope, &scope, false, walked_tx, Arc::clone(&probes));
    parked.recv_timeout(Duration::from_secs(30)).expect("the walk parks at its second directory");
    assert!(s.index_bytes() > 0, "the parked walk's arena already holds rows");
    let arena = Arc::downgrade(&s.index);
    assert_eq!(s.close(), 0, "a detached walk frees nothing yet");
    assert!(arena.upgrade().is_some(), "the running walk still holds the arena");
    let _ = release.send(());
    let start = Instant::now();
    while arena.upgrade().is_some() {
        assert!(start.elapsed() < Duration::from_secs(30), "the detached walk exits after release");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn search_memtime_watch_registration_has_no_mount_proportional_allocations() {
    use crate::backend::searchprior::alloc_tests::count_allocations;
    const UNRELATED_MOUNTS: usize = 256;
    // Permit bounded watch bookkeeping while excluding the measured 1,809-allocation mount reparse.
    const MAX_WATCH_ALLOCATIONS: usize = 32;
    let d = TestDir::new("walkmountalloc");
    let child = d.dir("watched");
    let mut body = String::from("1 0 8:1 / / rw - ext4 /dev/a rw\n");
    for i in 0..UNRELATED_MOUNTS {
        body.push_str(&format!("{} 1 8:2 / /unrelated/{i} rw - ext4 /dev/b rw\n", i + 2));
    }
    let reader = super::super::searchwalkread::Reader::new(&body);
    let probes = WalkProbes::new();
    let (tx, _rx) = crate::backend::events::channel();
    let watch = super::super::searchwatch::Watch::new(d.path(), false, tx);
    *probes.watch.lock().unwrap() = Some(Arc::clone(&watch.core));
    let index = Mutex::new(Index::new());
    let (opened, allocations) = count_allocations(|| {
        open_directory(&index, d.path(), &child, None, false, Some(&body), &reader, &probes)
    });
    assert!(opened.is_some());
    eprintln!("SEARCH_WALK_MOUNT_ALLOCATIONS mounts={} allocations={allocations}", UNRELATED_MOUNTS + 1);
    assert!(allocations <= MAX_WATCH_ALLOCATIONS, "the walk already parsed mounts; watch registration reparsed them with {allocations} allocations");
    d.file("watched/new", "");
    watch.core.flush();
    let delta = watch.core.drain();
    assert!(!delta.failed, "a registration failure cannot stand in for the child event");
    assert!(delta.dirs.contains(Path::new("watched")), "the allocation bound must still register the child watch");
}
