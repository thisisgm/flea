//! A card frame the shell does not own leaves its cached view alone, and the user's own fold lands in one frame.
use super::atomic_tests::{advance, drawn, finish, open_with, start};
use crate::{
    backend::{Backend, Event},
    window::root::Root,
};
use gpui::{AppContext, TestAppContext, WindowHandle};

fn live_line(cx: &mut TestAppContext, handle: WindowHandle<Root>, line: &str) {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.receive(0, Event::Operations(line.into()), window, cx);
            });
        })
        .unwrap();
}

fn fold(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    handle
        .update(cx, |root, _, cx| {
            let view = gpui::Entity::read(&root.shell, cx)
                .operation_view(0)
                .unwrap();
            view.update(cx, |overlay, cx| overlay.fold_jobs(cx));
        })
        .unwrap();
}

// A test window has no frame loop, so each spiral frame is delivered by hand and then drawn.
fn spiral_frame(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    let delivered = cx
        .update_window(handle.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })
        .unwrap();
    assert!(delivered > 0, "a running spiral asks for its next frame");
    assert!(drawn(cx, handle));
}

#[gpui::test]
fn u13_fix2_a_live_spiral_never_renders_the_shell(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    live_line(
        cx,
        handle,
        r#"{"t":"transferstarted","id":13,"n":1,"moving":false}"#,
    );
    live_line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":13,"kind":"copy","head":"Copying a.bin","state":"running","done":0,"total":1}}"#,
    );
    advance(cx, handle, 150);
    assert!(drawn(cx, handle), "the Jobs card is open on a running job");
    for _ in 0..4 {
        spiral_frame(cx, handle);
    }
    // Both ready surfaces are mounted, so real cached Shell layout must advance these existing counters.
    let list = handle
        .update(cx, |root, _, cx| {
            let shell = gpui::Entity::read(&root.shell, cx);
            let host = shell.panes[0].as_ref().unwrap();
            assert!(host.drawn_list);
            assert_eq!(host.drawn_view, Some(crate::window::startup::View::List));
            host.list.clone()
        })
        .unwrap();
    let frames = || {
        let list = list.borrow();
        (list.header.drawn.frames, list.drawn.frames)
    };
    let settled = frames();
    assert!(
        settled.0 > 0 && settled.1 > 0,
        "actual header and List have both been mounted"
    );
    for _ in 0..40 {
        advance(cx, handle, 8);
        spiral_frame(cx, handle);
    }
    assert_eq!(
        frames(),
        settled,
        "40 spiral frames with no status change leave the shell unrendered"
    );
    live_line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":13,"kind":"copy","head":"Copied a.bin","state":"done","done":1,"total":1}}"#,
    );
    live_line(
        cx,
        handle,
        r#"{"t":"transferdone","id":13,"ok":1,"failed":0}"#,
    );
    advance(cx, handle, 100);
    drawn(cx, handle);
    assert!(
        frames().0 > settled.0 && frames().1 > settled.1,
        "a real status change still renders the shell"
    );
}

#[gpui::test]
fn u13_fix2_unfolding_a_running_card_lands_in_one_frame(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    advance(cx, handle, 150);
    assert!(drawn(cx, handle));
    advance(cx, handle, 250);
    fold(cx, handle);
    assert!(!drawn(cx, handle), "the fold leaves in one frame");
    advance(cx, handle, 400);
    fold(cx, handle);
    assert!(
        drawn(cx, handle),
        "the unfold draws in one frame, with no 150 ms hold"
    );
}

#[gpui::test]
fn u13_fix2_a_fold_within_the_minimum_leaves_at_once(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    advance(cx, handle, 150);
    assert!(drawn(cx, handle));
    advance(cx, handle, 10);
    fold(cx, handle);
    assert!(
        !drawn(cx, handle),
        "10 ms after the first draw is inside the minimum, but the fold is the user's own"
    );
    fold(cx, handle);
    assert!(drawn(cx, handle), "the unfold returns in one frame too");
    advance(cx, handle, 20);
    finish(cx, handle);
    assert!(
        drawn(cx, handle),
        "a result still waits out its own minimum"
    );
}
