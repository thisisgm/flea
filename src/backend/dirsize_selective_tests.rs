use super::*;
use std::sync::atomic::AtomicUsize;

#[test]
fn fx400_selective_cancel_keeps_the_running_walk_and_skips_only_departed_paths() {
    let (events, rx) = crate::backend::events::channel();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let mut worker = Worker::with_walk(events, move |path, cancelled| {
        seen.fetch_add(1, Ordering::Relaxed);
        if path == Path::new("visible") {
            entered.send(()).unwrap();
            released.recv().unwrap();
        }
        assert_ne!(path, Path::new("departed"));
        dirsize::DirSize {
            bytes: 7,
            partial: cancelled(),
        }
    });
    worker.start(vec![
        (0, "visible".into()),
        (1, "departed".into()),
        (2, "also-visible".into()),
    ]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.cancel_rows(&[1]);
    assert!(worker.contains(0));
    assert!(!worker.contains(1));
    assert!(worker.contains(2));
    release.send(()).unwrap();
    for row in 0..3 {
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else {
            panic!()
        };
        assert_eq!(done.row, row);
        assert_eq!(worker.accept(&done), row != 1);
        assert_eq!(done.result.partial, row == 1);
    }
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(!worker.busy());
}

#[test]
fn fx400_selective_cancel_stops_the_running_row_without_stopping_its_neighbour() {
    let (events, rx) = crate::backend::events::channel();
    let (entered, entry) = channel();
    let (release, released) = channel();
    let mut worker = Worker::with_walk(events, move |path, cancelled| {
        if path == Path::new("departed") {
            entered.send(()).unwrap();
            released.recv().unwrap();
            assert!(cancelled());
        }
        dirsize::DirSize {
            bytes: 9,
            partial: cancelled(),
        }
    });
    worker.start(vec![(0, "departed".into()), (1, "visible".into())]);
    entry.recv_timeout(Duration::from_secs(2)).unwrap();
    worker.cancel_rows(&[0]);
    assert!(worker.busy());
    release.send(()).unwrap();
    for row in 0..2 {
        let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else {
            panic!()
        };
        assert_eq!(done.row, row);
        assert_eq!(worker.accept(&done), row == 1);
    }
    assert!(!worker.busy());
    worker.start(vec![(0, "fresh".into())]);
    let Event::DirSize(done) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else {
        panic!()
    };
    assert!(worker.accept(&done));
}

#[test]
fn fx400_selective_cancel_rejects_a_finished_but_unpublished_answer() {
    let (events, rx) = crate::backend::events::channel();
    let mut worker = Worker::with_walk(events, |_, _| dirsize::DirSize {
        bytes: 123,
        partial: false,
    });
    worker.start(vec![(0, "departed".into()), (1, "visible".into())]);
    let Event::DirSize(old) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else {
        panic!()
    };
    assert!(!old.result.partial);
    worker.cancel_rows(&[0]);
    assert!(
        !worker.accept(&old),
        "a completed result must not populate caches after its row left"
    );
    let Event::DirSize(kept) = rx.recv_timeout(Duration::from_secs(2)).unwrap() else {
        panic!()
    };
    assert!(worker.accept(&kept));
    assert!(!worker.busy());
}
