//! Withholding is guaranteed only before input is handed off, independent of thread scheduling.
use super::*;

#[test]
fn withheld_input_closes_stdin_empty() {
    let mut job = Job::plain(&["sh", "-c", "cat; echo done"]);
    job.stdin = Some(Secret::from_bytes(b"withheld-canary"));
    let (handle, send) = Handle::detached();
    handle.withhold_input();
    run(job, &handle.control, &send);
    assert!(matches!(handle.events.try_recv(), Ok(Event::Started)));
    let Event::Exited(outcome) = handle.events.try_recv().unwrap() else {
        panic!("the runner must report an exit");
    };
    assert_eq!(outcome.code, 0);
    assert_eq!(
        outcome.stdout, "done\n",
        "withholding closes stdin with empty EOF"
    );
    assert!(outcome.stderr.is_empty());
}
