use super::*;

const CANCEL_REPLY_WAIT: Duration = Duration::from_secs(2);

#[test]
fn adv_e_r1_cancelling_queued_office_answers_before_the_running_job_finishes() {
    let d = TestDir::new("officepdf-queued-cancel");
    let r = rig(&d, GATED, CONVERT_TIMEOUT);
    let gate = d.join("running.docx");
    mkfifo(&gate);
    assert_eq!(r.ask(1, &gate), converting_line(1));
    let end = Instant::now() + BOUND;
    while r.jobs() == 0 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(r.jobs(), 1, "the unrelated converter is running before cancellation");
    let source = d.file("queued.docx", "office");
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&cancel);
    let service = Arc::clone(&r.svc);
    let (tx, rx) = channel();
    let fetch = std::thread::spawn(move || {
        tx.send(service.fetch(&source, stop)).unwrap();
    });
    let end = Instant::now() + BOUND;
    while r.svc.locked().required.is_empty() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
    }
    let queued_before = r.svc.locked().required.len();
    cancel.store(true, Ordering::Relaxed);
    let response_before_release = rx.recv_timeout(CANCEL_REPLY_WAIT);
    let queued_after = r.svc.locked().required.len();
    let jobs_before_release = r.jobs();
    within("the running office conversion is released", move || {
        drop(std::fs::OpenOptions::new().write(true).open(&gate).unwrap());
    });
    fetch.join().unwrap();
    assert!(r.end().1.contains("ready"));
    assert_eq!(queued_before, 1, "the cancelled request was queued behind the running job");
    assert_eq!(response_before_release.unwrap(), Err("cancelled".into()), "cancellation must answer before the unrelated conversion ends");
    assert_eq!(queued_after, 0, "the cancelled request must leave the queue");
    assert_eq!(jobs_before_release, 1, "the unrelated conversion was still running");
    assert_eq!(r.jobs(), 1, "a cancelled queued request never starts a converter");
    assert_eq!(r.fail_markers(), 0);
}
