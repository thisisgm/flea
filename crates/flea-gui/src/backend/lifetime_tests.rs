//! Final child events must finish inside the real host fixture's executor lifetime.
use super::super::*;
use gpui::{TestAppContext, TestDispatcher};

#[test]
fn fx456_fixture_teardown_does_not_wake_a_retired_scheduler() {
    let dispatcher = TestDispatcher::new(0);
    let scheduler = dispatcher.scheduler().clone();
    let mut cx = TestAppContext::build(dispatcher, Some("fx456 real plugin host fixture"));
    let mut command = Command::new("/bin/sh");
    command.args([
        "-c",
        r#"while IFS= read -r line; do :; done
printf '%s\n' '{"t":"plugins","id":0,"plugins":[]}'
"#,
    ]);
    let backend = Backend::start_with(command, r#"{"c":"noop"}"#.into()).unwrap();
    let joined = backend.test_own(&mut cx).unwrap();
    let drained = backend.drained();
    let replies = backend.replies.clone();
    let host = cx.update(crate::g2::plugins::service);
    host.update(&mut cx, |host, cx| host.test_attach(backend, cx));
    cx.run_until_parked();
    // EOF is the handoff: the child cannot publish its final event until fixture shutdown.
    cx.quit();
    drained.recv_blocking().unwrap();
    assert!(joined.load(std::sync::atomic::Ordering::SeqCst));
    cx.run_until_parked();
    assert_eq!(
        replies.receiver_count(),
        1,
        "fixture receiver task outlived teardown"
    );
    drop(cx);
    scheduler.end_test();
}

#[test]
fn fx456_completion_preserves_drained_before_and_after_another_owner_receives_it() {
    for receive_first in [false, true] {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "while IFS= read -r line; do :; done"]);
        let backend = Backend::start_with(command, r#"{"c":"noop"}"#.into()).unwrap();
        let completion = backend.test_completion().unwrap();
        let joined = completion.joined.clone();
        let drained = backend.drained();
        backend.close_stdin();
        if receive_first {
            drained.recv_blocking().unwrap();
        }
        drop(completion);
        assert!(joined.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(backend.closing_probe()(), Closing::Stdin);
        if !receive_first {
            drained
                .try_recv()
                .expect("fixture completion must not consume drained");
        }
    }
}

#[test]
fn fx456_shutdown_cannot_discard_a_worker_failure() {
    let mut command = Command::new("/bin/sh");
    // The child leaves after its second line, so the worker fails only after replies close.
    command.args(["-c", "read -r first; read -r second"]);
    let backend = Backend::start_with(command, r#"{"c":"noop"}"#.into()).unwrap();
    let worker = backend.worker.lock().unwrap().take().unwrap();
    backend.replies.close();
    backend.line(r#"{"c":"noop"}"#.into()).unwrap();
    let panic = worker
        .join()
        .expect_err("worker failure was silently discarded");
    let message = panic.downcast_ref::<String>().unwrap();
    assert!(message.contains("window.backend_eof"), "{message}");
}
