//! The operation card's real paint and IPC must measure recent published bytes, even without wire beats.
use super::{draw, state};
use crate::{
    backend::{Backend, Event},
    window::root::Root,
};
use gpui::{AppContext, TestAppContext, WindowHandle};
use std::time::Duration;

fn line(cx: &mut TestAppContext, handle: WindowHandle<Root>, text: &str) {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.receive(0, Event::Operations(text.into()), window, cx);
                if let Some(view) = shell.operation_view(0) {
                    view.update(cx, |overlay, _| overlay.pinned = Some(0.));
                }
            });
        })
        .unwrap();
}

fn bytes(cx: &mut TestAppContext, handle: WindowHandle<Root>, bytes: u64) {
    line(
        cx,
        handle,
        &format!(
            r#"{{"t":"job","job":{{"id":12,"kind":"copy","head":"Copying 1 of 2","file":"a.bin","done":0,"total":2,"bytes":{bytes},"totalBytes":100000000,"rate":99000000,"left":1,"state":"running"}}}}"#
        ),
    );
}

fn advance(cx: &mut TestAppContext, handle: WindowHandle<Root>, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
}

fn shown(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> String {
    let seen = state(cx, handle);
    let card = seen.get("transferCard").unwrap();
    assert_eq!(card.get("visible").unwrap().as_bool(), Some(true));
    let text = card.get("byteLine").unwrap().as_str().unwrap().to_owned();
    handle
        .update(cx, |root, _, cx| {
            let view = root.shell.read(cx).operation_view(0).unwrap();
            let overlay = view.read(cx);
            let painted = overlay.feedback_paint.get("byte-line").unwrap();
            assert_eq!(painted.text.to_string(), text);
            assert_eq!(
                overlay.model.jobs.rows[0].rate, 99_000_000,
                "wire average stays untouched"
            );
            assert_eq!(overlay.model.jobs.rows[0].left, Some(1));
        })
        .unwrap();
    text
}

fn start(cx: &mut TestAppContext) -> (WindowHandle<Root>, impl FnMut() -> Option<String>) {
    let (backend, received) = Backend::test_lines();
    let handle = cx.add_window(|_, cx| Root {
        shell: cx.new(|cx| crate::state::tests::shell_with(backend, cx)),
    });
    draw(cx, handle);
    line(
        cx,
        handle,
        r#"{"t":"transferstarted","id":12,"n":2,"moving":false}"#,
    );
    bytes(cx, handle, 1_000_000);
    advance(cx, handle, crate::pane::swap::HOLD_MS);
    (handle, received)
}

#[gpui::test]
fn card_rate_stall_decays_without_any_backend_events(cx: &mut TestAppContext) {
    let (handle, _received) = start(cx);
    bytes(cx, handle, 11_000_000);
    advance(cx, handle, 250);
    for _ in 0..9 {
        advance(cx, handle, 250);
    }
    assert_eq!(
        shown(cx, handle),
        "11.0 MB of 100.0 MB · 0 B/s",
        "stalled bytes must lose both stale speed and ETA"
    );
}

#[gpui::test]
fn card_rate_burst_publishes_on_beat_and_resume_replaces_stall(cx: &mut TestAppContext) {
    let (handle, _received) = start(cx);
    assert_eq!(shown(cx, handle), "1.0 MB of 100.0 MB · 0 B/s");
    bytes(cx, handle, 11_000_000);
    draw(cx, handle);
    assert_eq!(
        shown(cx, handle),
        "1.0 MB of 100.0 MB · 0 B/s",
        "wire burst is not a publication"
    );
    advance(cx, handle, 249);
    assert_eq!(shown(cx, handle), "1.0 MB of 100.0 MB · 0 B/s");
    advance(cx, handle, 1);
    assert_eq!(
        shown(cx, handle),
        "11.0 MB of 100.0 MB · 40.0 MB/s · 0:02 left"
    );
    for _ in 0..9 {
        advance(cx, handle, 250);
    }
    assert_eq!(shown(cx, handle), "11.0 MB of 100.0 MB · 0 B/s");
    bytes(cx, handle, 20_000_000);
    advance(cx, handle, 250);
    assert_eq!(
        shown(cx, handle),
        "20.0 MB of 100.0 MB · 4.0 MB/s · 0:20 left"
    );
}

#[gpui::test]
fn card_rate_identical_deadline_reply_cannot_cancel_stall_wakeup(cx: &mut TestAppContext) {
    let (handle, _received) = start(cx);
    bytes(cx, handle, 11_000_000);
    advance(cx, handle, 250);
    assert_eq!(
        shown(cx, handle),
        "11.0 MB of 100.0 MB · 40.0 MB/s · 0:02 left"
    );
    // Move only the clock, so the identical wire reply wins the race with the ready timer callback.
    cx.executor()
        .dispatcher()
        .as_test()
        .unwrap()
        .scheduler()
        .clock()
        .advance(Duration::from_millis(250));
    bytes(cx, handle, 11_000_000);
    handle
        .update(cx, |root, _, cx| {
            let view = root.shell.read(cx).operation_view(0).unwrap();
            let overlay = view.read(cx);
            assert!(!overlay.model.changed, "the duplicate really is unchanged");
            assert_eq!(overlay.model.due(), None);
            assert!(
                overlay.model.jobs.visible(),
                "show_operation's guard admits this running job"
            );
            assert!(overlay.model.wire.transfer.running);
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    for _ in 0..9 {
        advance(cx, handle, 250);
    }
    assert_eq!(shown(cx, handle), "11.0 MB of 100.0 MB · 0 B/s");
}
