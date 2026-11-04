use super::*;

#[test]
fn queued_cancellation_reports_stopped_and_unexpected_disconnect_remains_a_failure() {
    for (cancelled, expected) in [
        (true, "plugin stopped"),
        (false, "plugin worker disconnected"),
    ] {
        let (requests, receive) = mpsc::sync_channel::<Call>(1);
        let stop = Arc::new(AtomicBool::new(false));
        let cancelled_stop = stop.clone();
        let worker = thread::spawn(move || {
            let call = receive.recv().unwrap();
            if cancelled {
                cancelled_stop.store(true, Ordering::Release);
            }
            drop(call);
        });
        let runtime = Runtime(Arc::new(Inner {
            requests,
            stop,
            busy: AtomicBool::new(false),
            failure: Arc::new(Mutex::new(None)),
            worker: Mutex::new(Some(worker)),
            timeout: Duration::from_secs(1),
        }));
        assert_eq!(runtime.ensure_ready().unwrap_err(), expected);
    }
}
