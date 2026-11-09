//! Progress timing is measured through the real root, rendered Jobs card and published status.
use crate::{
    backend::{Backend, Event},
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{AppContext, TestAppContext, VisualTestContext, WindowHandle};
use std::time::Duration;

pub(super) fn open_with(cx: &mut TestAppContext, backend: Backend) -> WindowHandle<Root> {
    cx.add_window(|window, cx| {
        let shell = cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            // These tests read every operation command in order, so the rows ask no thumbnails between them.
            shell.launch.settings.data =
                flea::jsondoc::parse(r#"{"preview":{"thumbnails":"off"}}"#).unwrap();
            // Ready listing2 gets independent footer facts and List local class "", without consuming or filtering commands.
            let host = shell.panes[0].as_mut().unwrap();
            host.sizes = crate::list::sizes::State::settled_fixture(
                std::rc::Rc::clone(&host.pane.held),
                host.path.clone(),
                "",
            );
            let facts = crate::backend::status::Filesystem {
                class: String::new(),
                tagable: None,
                path: host.path.clone(),
                name: "testfs".into(),
                free: 1_073_741_824,
            };
            shell.surfaces[0].filesystem =
                crate::window::status::Filesystem::settled_fixture(facts, host.pane.held.listing);
            shell
        });
        let focus = shell.read(cx).focus.clone();
        window.focus(&focus, cx);
        Root { shell }
    })
}

pub(super) fn line(cx: &mut TestAppContext, handle: WindowHandle<Root>, line: &str) {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                shell.receive(0, Event::Operations(line.into()), window, cx);
                if let Some(view) = shell.operation_view(0) {
                    view.update(cx, |overlay, _| overlay.pinned = Some(0.));
                }
            });
        })
        .unwrap();
}

pub(super) fn start(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    line(
        cx,
        handle,
        r#"{"t":"transferstarted","id":13,"n":1,"moving":false}"#,
    );
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":13,"kind":"copy","head":"Copying a.bin","state":"running","done":0,"total":1}}"#,
    );
}

pub(super) fn finish(cx: &mut TestAppContext, handle: WindowHandle<Root>) {
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":13,"kind":"copy","head":"Copied a.bin","state":"done","done":1,"total":1}}"#,
    );
    line(
        cx,
        handle,
        r#"{"t":"transferdone","id":13,"ok":1,"failed":0}"#,
    );
}

pub(super) fn advance(cx: &mut TestAppContext, handle: WindowHandle<Root>, ms: u64) {
    let visual = VisualTestContext::from_window(handle.into(), cx);
    visual.executor().advance_clock(Duration::from_millis(ms));
    visual.run_until_parked();
}

pub(super) fn drawn(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> bool {
    cx.update_window(handle.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("op1-jobs")
        .is_some()
}

fn status(cx: &mut TestAppContext, handle: WindowHandle<Root>) -> String {
    handle
        .update(cx, |root, _, cx| {
            let shell: &Shell = root.shell.read(cx);
            shell.panes[0].as_ref().unwrap().status.to_string()
        })
        .unwrap()
}

#[gpui::test]
fn u13_fix1_fast_job_never_draws_progress(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    assert!(!drawn(cx, handle), "a new progress card waits 150 ms");
    advance(cx, handle, 149);
    finish(cx, handle);
    assert!(
        !drawn(cx, handle),
        "a job finished within the hold never draws"
    );
    assert!(status(cx, handle).starts_with("Copied 1 item"));
    advance(cx, handle, 101);
    assert!(
        !drawn(cx, handle),
        "no stale timer flashes the finished job"
    );
}

#[gpui::test]
fn u13_fix1_slow_job_draws_at_150_ms(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    assert!(!drawn(cx, handle));
    advance(cx, handle, 149);
    assert!(!drawn(cx, handle), "149 ms remains inside the hold");
    advance(cx, handle, 1);
    assert!(drawn(cx, handle), "the host wakes and draws at 150 ms");
}

#[gpui::test]
fn u13_fix1_completion_waits_for_100_ms_of_drawn_progress(cx: &mut TestAppContext) {
    let (backend, _received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    advance(cx, handle, 150);
    assert!(drawn(cx, handle));
    advance(cx, handle, 20);
    finish(cx, handle);
    assert!(drawn(cx, handle), "20 ms is shorter than the drawn minimum");
    let mark = VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("op1-job-13-mark")
        .expect("the retained job mark must still be laid out");
    handle
        .update(cx, |root, window, cx| {
            let view = root.shell.read(cx).operation_view(0).unwrap();
            let overlay = view.read(cx);
            assert!(!overlay.model.jobs.visible());
            let frame = overlay.palette.muted_frame;
            let square = mark.scale(window.scale_factor());
            let painted = window.painted_quads().iter().any(|quad| {
                quad.bounds.is_contained_within(&square)
                    && quad
                        .background
                        .as_solid()
                        .is_some_and(|ink| (ink.h, ink.s, ink.l) == (frame.h, frame.s, frame.l))
            });
            assert!(
                painted,
                "the retained job spiral must actually paint its quads"
            );
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            root.shell.update(cx, |shell, cx| {
                shell.ipc(
                    crate::ipc::server::Event::Call {
                        request: flea::ipc::wire::Request::Call {
                            target: "flea".into(),
                            function: "omiState".into(),
                            arguments: vec![],
                        },
                        answer,
                    },
                    window,
                    cx,
                );
            });
            let reply = receive.try_recv().unwrap().reply;
            assert_eq!(reply.code, 0);
            let data = flea::jsondoc::parse(reply.stdout.trim()).unwrap();
            let row = data
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row.get("id").and_then(flea::jsondoc::Json::as_f64) == Some(13.))
                .expect("IPC must describe the retained rendered job");
            assert_eq!(
                row.get("shown").and_then(flea::jsondoc::Json::as_bool),
                Some(painted),
                "omiState must agree with the spiral painted in this frame"
            );
        })
        .unwrap();
    assert!(!status(cx, handle).starts_with("Copied 1 item"));
    advance(cx, handle, 79);
    assert!(drawn(cx, handle), "99 ms still holds the progress snapshot");
    assert!(!status(cx, handle).starts_with("Copied 1 item"));
    advance(cx, handle, 1);
    assert!(!drawn(cx, handle), "the result swaps in at 100 ms");
    assert!(status(cx, handle).starts_with("Copied 1 item"));
}

#[gpui::test]
fn u13_fix1_collision_focus_matches_the_board(cx: &mut TestAppContext) {
    let (backend, mut received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| {
                assert!(shell.ask_operation(0, &super::tests::request(), &[], false, window, cx));
            })
        })
        .unwrap();
    assert!(received().unwrap().contains("collisions"));
    line(
        cx,
        handle,
        r#"{"t":"collisions","id":1,"total":1,"names":[{"n":"notes","d":false,"i":""}],"mergeable":false}"#,
    );
    for (key, expected) in [
        ("", "keep"),
        ("tab", "cancel"),
        ("tab", "skip"),
        ("tab", "replace"),
        ("tab", "keep"),
        ("h", "replace"),
        ("h", "skip"),
    ] {
        if !key.is_empty() {
            let mut visual = VisualTestContext::from_window(handle.into(), cx);
            visual.simulate_keystrokes(key);
            visual.run_until_parked();
        }
        drawn(cx, handle);
        handle
            .update(cx, |root, _, cx| {
                let shell = root.shell.read(cx);
                let overlay = shell.operation_view(0).unwrap();
                let Some(super::model::Dialog::Collide { focus, .. }) =
                    overlay.read(cx).drawn_dialog()
                else {
                    panic!("the collision card must be drawn");
                };
                assert_eq!(focus.wire(), expected);
            })
            .unwrap();
    }
}

// The host's timer draws at 150 ms, so a late first draw is staged on the gate itself with a fake clock.
#[test]
fn u13_fix1_a_late_first_draw_starts_its_own_minimum() {
    let mut gate = crate::pane::swap::Progress::<bool>::default();
    gate.observe(&true, true, 0);
    assert!(!*gate.commit(149), "inside the hold nothing draws");
    assert!(*gate.commit(200), "the first draw is late, at 200 ms");
    gate.observe(&false, false, 220);
    assert!(*gate.commit(299), "99 ms after the first drawn frame holds");
    assert!(*gate.commit(299));
    assert!(
        !*gate.commit(300),
        "the minimum counts from the drawn frame"
    );
}

#[gpui::test]
fn u13_fix1_a_held_completed_job_cannot_be_cancelled_again(cx: &mut TestAppContext) {
    let (backend, mut received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    advance(cx, handle, 150);
    assert!(drawn(cx, handle));
    advance(cx, handle, 20);
    finish(cx, handle);
    assert!(drawn(cx, handle));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    let bounds = visual.debug_bounds("op1-job-13").unwrap();
    let point = gpui::point(
        bounds.right() - gpui::px(5.),
        bounds.bottom() - gpui::px(10.),
    );
    visual.simulate_mouse_move(point, None, gpui::Modifiers::default());
    visual.simulate_mouse_down(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.simulate_mouse_up(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    visual.run_until_parked();
    // The completion re-read is the listing v0.3.12 sends after transferdone (ui/PaneWire.qml:330-332); it is not a cancel.
    let sent: Vec<String> = std::iter::from_fn(&mut received).collect();
    assert!(
        !sent.iter().any(|line| line.contains("cancel")),
        "the producer already completed this id: {sent:?}"
    );
}

#[gpui::test]
fn u13_fix1_escape_cannot_cancel_an_unseen_new_job(cx: &mut TestAppContext) {
    let (backend, mut received) = Backend::test_operation_channel();
    let handle = open_with(cx, backend);
    start(cx, handle);
    advance(cx, handle, 150);
    assert!(drawn(cx, handle));
    advance(cx, handle, 20);
    finish(cx, handle);
    advance(cx, handle, 10);
    line(
        cx,
        handle,
        r#"{"t":"transferstarted","id":14,"n":1,"moving":false}"#,
    );
    line(
        cx,
        handle,
        r#"{"t":"job","job":{"id":14,"kind":"copy","head":"Copying b.bin","state":"running","done":0,"total":1}}"#,
    );
    assert!(drawn(cx, handle));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    // The completion re-read is the listing v0.3.12 sends after transferdone (ui/PaneWire.qml:330-332); it is not a cancel.
    let sent: Vec<String> = std::iter::from_fn(&mut received).collect();
    assert!(
        !sent.iter().any(|line| line.contains("cancel")),
        "the drawn job finished and the replacement has not drawn: {sent:?}"
    );
    advance(cx, handle, 149);
    drawn(cx, handle);
    assert!(VisualTestContext::from_window(handle.into(), cx)
        .debug_bounds("op1-job-14")
        .is_none());
    advance(cx, handle, 1);
    assert!(drawn(cx, handle));
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_keystrokes("escape");
    visual.run_until_parked();
    assert_eq!(
        received().as_deref(),
        Some(r#"{"c":"transfercancel","id":14}"#)
    );
    assert!(received().is_none());
}

#[path = "jobs_visibility_tests.rs"]
mod jobs_visibility_tests;

#[path = "uish_escape_tests.rs"]
mod uish_escape_tests;
