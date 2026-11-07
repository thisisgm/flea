use super::*;
// Depth 1900 exceeds the 1024 open-file limit of a per-level-descriptor traversal like remove_dir_all, so cleanup runs on every exit including panic while the walk itself holds one listing (see dirsize.rs).
const DEEP_DEPTH: usize = 1900;

struct DeepTree {
    sandbox: super::super::testdir::TestDir,
    leaf: PathBuf,
}

impl DeepTree {
    // Depth stays what the stack proof was taken at: under PATH_MAX as paths, over any real tree.
    fn build(tag: &str) -> Self {
        let sandbox = super::super::testdir::TestDir::new(tag);
        let mut leaf = sandbox.path().to_path_buf();
        for _ in 0..DEEP_DEPTH {
            leaf.push("d");
            sandbox.assert_contains(&leaf);
            std::fs::create_dir(&leaf).unwrap();
        }
        Self { sandbox, leaf }
    }
    fn root(&self) -> &std::path::Path {
        self.sandbox.path()
    }
}

impl Drop for DeepTree {
    fn drop(&mut self) {
        // Bottom up, one rmdir at a time; whatever this cannot take falls through to the sandbox drop.
        while self.leaf != self.sandbox.path() {
            if std::fs::remove_dir(&self.leaf).is_err() {
                break;
            }
            self.leaf.pop();
        }
    }
}

// A test dying before manual cleanup leaves its tree to the sandbox drop, whose remove_dir_all holds a descriptor per level.
#[test]
fn a_deep_tree_is_removed_even_when_its_test_fails_before_cleanup() {
    let slot: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);
    // No invariant crosses the panic: the slot write completes before it.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let tree = DeepTree::build("size-deep-unwind");
        *slot.lock().unwrap() = Some(tree.root().to_path_buf());
        panic!("simulated failure before cleanup");
    }));
    assert!(result.is_err(), "the simulated failure did not panic");
    let kept = slot.lock().unwrap().take().unwrap();
    assert!(!kept.exists(), "a failed deep test left its tree behind: {}", kept.display());
}

#[test]
fn deep_directory_tree_fits_the_worker_stack() {
    let tree = DeepTree::build("size-deep");
    let (events, rx) = crate::backend::events::channel();
    let mut worker = Worker::new(events);
    worker.start(vec![(0, tree.root().to_path_buf())]);
    let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(10)).unwrap() else { panic!() };
    assert!(worker.accept(&done));
    assert!(done.result.bytes > 0);
    assert!(!done.result.partial, "the walk reached the bottom of the tree rather than stopping short of it");
}

#[test]
fn cancelled_running_job_cannot_publish_to_reused_row() {
    let (events, rx) = crate::backend::events::channel();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let mut worker = Worker::with_walk(events, move |_, cancelled| {
        entered.send(()).unwrap();
        released.recv().unwrap();
        dirsize::DirSize { bytes: 42, partial: cancelled() }
    });
    worker.start(vec![(0, PathBuf::new())]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.cancel();
    assert!(worker.busy());
    assert!(!worker.contains(0));
    release.send(()).unwrap();
    let Event::DirSize(old) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert!(old.result.partial);
    assert!(!worker.accept(&old));
    assert!(!worker.busy());
    worker.start(vec![(0, PathBuf::new())]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!worker.accept(&old));
    assert!(worker.busy());
    release.send(()).unwrap();
    let Event::DirSize(new) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert!(!new.result.partial);
    assert!(worker.accept(&new));
    assert!(!worker.busy());
}

#[test]
fn a_completed_row_does_not_release_later_rows_in_same_batch() {
    let (events, rx) = crate::backend::events::channel();
    let (release, released) = channel();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let mut worker = Worker::with_walk(events, move |_, _| {
        if seen.fetch_add(1, Ordering::Relaxed) == 1 {
            released.recv().unwrap();
        }
        dirsize::DirSize { bytes: 7, partial: false }
    });
    worker.start(vec![(0, PathBuf::from("first")), (1, PathBuf::from("second"))]);
    let Event::DirSize(first) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert!(worker.accept(&first));
    assert!(worker.busy());
    release.send(()).unwrap();
    let Event::DirSize(second) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert!(worker.accept(&second));
    assert!(!worker.busy());
}

#[test]
fn cancellation_skips_queued_paths_and_fresh_batch_progresses() {
    let (events, rx) = crate::backend::events::channel();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let mut worker = Worker::with_walk(events, move |_, cancelled| {
        let call = seen.fetch_add(1, Ordering::Relaxed);
        if call == 0 {
            entered.send(()).unwrap();
            released.recv().unwrap();
        }
        dirsize::DirSize { bytes: 9, partial: cancelled() }
    });
    worker.start(vec![(0, PathBuf::from("running")), (1, PathBuf::from("skipped"))]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.cancel();
    release.send(()).unwrap();
    for _ in 0..2 {
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(!worker.accept(&done));
    }
    assert!(!worker.busy());
    assert_eq!(calls.load(Ordering::Relaxed), 1);

    worker.start(vec![(2, PathBuf::from("fresh"))]);
    let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert!(worker.accept(&done));
    assert!(!worker.busy());
}

#[test]
fn duplicate_done_cannot_release_or_reuse_a_batch_slot() {
    let (events, rx) = crate::backend::events::channel();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let mut worker = Worker::with_walk(events, move |_, _| {
        if seen.fetch_add(1, Ordering::Relaxed) == 1 {
            entered.send(()).unwrap();
            released.recv().unwrap();
        }
        dirsize::DirSize { bytes: 11, partial: false }
    });
    worker.start(vec![(0, PathBuf::from("old"))]);
    let Event::DirSize(old) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert!(worker.accept(&old));
    worker.start(vec![(0, PathBuf::from("new"))]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(!worker.accept(&old));
    assert!(worker.busy());
    release.send(()).unwrap();
    let Event::DirSize(new) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert!(worker.accept(&new));
    assert!(!worker.busy());
}

#[test]
fn a_stuck_batch_is_replaced_so_sizes_resume() {
    let (events, rx) = crate::backend::events::channel();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let mut worker = Worker::with_walk(events, move |path: &Path, _| {
        if path == Path::new("stuck") {
            entered.send(()).unwrap();
            released.recv().unwrap();
        }
        dirsize::DirSize { bytes: 7, partial: false }
    });
    worker.start(vec![(0, PathBuf::from("stuck"))]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(worker.busy(), "a running walk holds the slot");
    worker.force_past_deadline();
    assert!(worker.stuck(), "past the deadline the batch counts as stuck");
    assert!(!worker.busy(), "a stuck batch releases the slot for a fresh worker");
    worker.start(vec![(1, PathBuf::from("fresh"))]);
    let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert_eq!(done.row, 1, "the fresh worker answers while the stuck thread is still held");
    assert!(worker.accept(&done));
    release.send(()).unwrap();
}

#[test]
fn a_healthy_batch_restarts_its_clock_per_row() {
    // Sample input: batch start forced past the deadline, then one row lands, so stuck clears.
    let (events, rx) = crate::backend::events::channel();
    let (release, released) = channel();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let mut worker = Worker::with_walk(events, move |_, _| {
        if seen.fetch_add(1, Ordering::Relaxed) == 1 {
            released.recv().unwrap();
        }
        dirsize::DirSize { bytes: 7, partial: false }
    });
    worker.start(vec![(0, PathBuf::from("first")), (1, PathBuf::from("second"))]);
    let Event::DirSize(first) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    worker.force_past_deadline();
    assert!(worker.stuck(), "a batch past its deadline counts as stuck");
    assert!(worker.accept(&first), "the first row still publishes");
    assert!(!worker.stuck(), "one answered row restarts the clock for the next walk");
    assert!(worker.busy(), "the batch still holds the slot for its second row");
    release.send(()).unwrap();
    let Event::DirSize(second) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
    assert!(worker.accept(&second));
    assert!(!worker.busy());
}

#[test]
fn a_cancelled_batch_keeps_its_clock_so_a_wedge_still_times_out() {
    // Sample input: cancel with a walk in flight keeps started, so the wedge becomes stuck.
    let (events, _rx) = crate::backend::events::channel();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let mut worker = Worker::with_walk(events, move |_, _| {
        entered.send(()).unwrap();
        released.recv().unwrap();
        dirsize::DirSize { bytes: 7, partial: false }
    });
    worker.start(vec![(0, PathBuf::from("wedged"))]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.cancel();
    assert!(worker.started.is_some(), "cancel leaves the clock running for the wedged walk");
    assert!(!worker.stuck(), "a fresh cancel is not yet stuck");
    worker.force_past_deadline();
    assert!(worker.stuck(), "the cancelled wedge still becomes stuck and is replaced");
    assert!(!worker.busy(), "so the slot releases for a fresh worker");
    release.send(()).unwrap();
}

#[test]
fn a_dead_mount_holds_one_thread_until_its_walk_returns() {
    // Sample input: mark 61 s old with its thread still out stays, with its thread back goes.
    let (events, _rx) = crate::backend::events::channel();
    let mut worker = Worker::with_walk(events, |_, _| dirsize::DirSize { bytes: 1, partial: false });
    let probe = PathBuf::from("/media/stick");
    let mount = mount_key(&probe);
    let live = Arc::new(AtomicBool::new(false));
    worker.stuck.insert(mount.clone(), (Instant::now() - Duration::from_secs(STUCK_TTL_SECS + 1), Arc::clone(&live)));
    worker.prune_stuck();
    assert!(worker.stuck.contains_key(&mount), "a thread still out keeps its mark past the TTL");
    assert!(worker.stuck_skips(&probe), "so the dead mount is still skipped");
    live.store(true, Ordering::Relaxed);
    worker.prune_stuck();
    assert!(!worker.stuck.contains_key(&mount), "a returned thread lets the TTL prune the mark");
    assert!(!worker.stuck_skips(&probe), "so one walk tries the mount again");
}

#[test]
fn a_batch_just_past_its_deadline_is_not_stuck_until_its_grace_passes() {
    // Sample input: started DEADLINE_MS plus 100 ms answers false, plus the grace answers true.
    let (events, _rx) = crate::backend::events::channel();
    let mut worker = Worker::with_walk(events, |_, _| dirsize::DirSize { bytes: 1, partial: false });
    worker.start(vec![(0, PathBuf::from("slow"))]);
    worker.started = Some(Instant::now() - Duration::from_millis(dirsize::DEADLINE_MS + 100));
    assert!(!worker.stuck(), "one slow readdir past the deadline is grace, not a wedge");
    worker.started = Some(Instant::now() - Duration::from_millis(dirsize::DEADLINE_MS + STUCK_GRACE_MS + 100));
    assert!(worker.stuck(), "past the grace the batch counts as stuck");
}

#[test]
fn an_abandoned_batch_sets_its_mount_flag_once_its_walk_returns() {
    // Sample input: two-row batch wedged on row 0, abandoned, then both stale Dones accepted.
    let (events, rx) = crate::backend::events::channel();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let mut worker = Worker::with_walk(events, move |_, _| {
        if seen.fetch_add(1, Ordering::Relaxed) == 0 {
            entered.send(()).unwrap();
            released.recv().unwrap();
        }
        dirsize::DirSize { bytes: 7, partial: false }
    });
    let first = PathBuf::from("wedged-first");
    let second = PathBuf::from("wedged-second");
    worker.start(vec![(0, first.clone()), (1, second)]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.force_past_deadline();
    assert!(worker.stuck(), "the wedged batch counts as stuck");
    worker.start(vec![(2, PathBuf::from("fresh"))]);
    release.send(()).unwrap();
    let mut stale = 0;
    for _ in 0..3 {
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        if done.row == 2 {
            assert!(worker.accept(&done), "the fresh row still publishes");
        } else {
            assert!(!worker.accept(&done), "a stale row never publishes");
            stale += 1;
        }
    }
    assert_eq!(stale, 2, "both wedged rows answer once the walk returns");
    let mount = mount_key(&first);
    let done = worker.stuck.get(&mount).map(|(_, flag)| flag.load(Ordering::Relaxed)).unwrap_or(false);
    assert!(done, "two stale Dones store the mount flag the mark owns");
    worker.stuck.get_mut(&mount).unwrap().0 = Instant::now() - Duration::from_secs(STUCK_TTL_SECS + 1);
    worker.prune_stuck();
    assert!(!worker.stuck.contains_key(&mount), "a returned thread lets the TTL prune the mark");
    assert!(!worker.stuck_skips(&first), "so one walk tries the mount again");
}

#[test]
fn duplicate_rows_are_walked_once_and_all_unique_rows_release_batch() {
    let (events, rx) = crate::backend::events::channel();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let mut worker = Worker::with_walk(events, move |_, _| {
        seen.fetch_add(1, Ordering::Relaxed);
        dirsize::DirSize { bytes: 13, partial: false }
    });
    worker.start(vec![
        (0, PathBuf::from("first")),
        (0, PathBuf::from("duplicate")),
        (1, PathBuf::from("second")),
        (1, PathBuf::from("duplicate")),
    ]);
    for _ in 0..2 {
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else { panic!() };
        assert!(worker.accept(&done));
    }
    assert!(rx.try_recv().is_err());
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(!worker.busy());
}
