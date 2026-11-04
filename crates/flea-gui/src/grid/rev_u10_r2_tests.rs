//! Review proofs for uish-u10 round 2; each asserts the behaviour the brief and round 1 ask for.
use super::join_tests::{frame, named_shell};
use crate::{backend::Backend, state::Shell, window::startup::View};
use gpui::{KeyDownEvent, Keystroke, TestAppContext, WindowHandle};

// Presses a key the way the window does, then lets GPUI draw only what it schedules itself.
fn press(cx: &mut TestAppContext, handle: WindowHandle<Shell>, name: &str) {
    handle
        .update(cx, |shell, window, cx| {
            shell.key(
                &KeyDownEvent {
                    keystroke: Keystroke::parse(name).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx,
            );
        })
        .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn rev_u10_r2_a_failed_grid_send_paints_the_error_surface_without_another_input(
    cx: &mut TestAppContext,
) {
    let (backend, requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    drop(requests);
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    press(cx, handle, "end");
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let kind = host.pane.state.kind;
            let tiles = host.grid.as_ref().unwrap().state.borrow().drawn().len();
            let surface = shell.slots.states[0].is_some();
            assert_eq!(kind, crate::pane::states::Kind::Error, "the send failed");
            assert!(
                surface && tiles == 0,
                "the pane is {kind:?} but the last frame GPUI drew is the Ready Grid ({tiles} tiles painted, error surface built {surface}), and nothing schedules another frame"
            );
        })
        .unwrap();
}

#[gpui::test]
fn rev_u10_r2_a_reload_during_a_staged_switch_never_lands_a_half_drawn_list(
    cx: &mut TestAppContext,
) {
    let (backend, _requests) = Backend::test_channel();
    let handle = cx.add_window(|_, cx| named_shell(backend, cx));
    handle
        .update(cx, |shell, window, cx| {
            shell.set_view(View::Grid, window, cx)
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, window, cx| {
            shell.panes[0].as_mut().unwrap().pane.cursor = 999;
            shell.set_view(View::List, window, cx);
            assert_eq!(
                shell.view,
                View::Grid,
                "the switch is staged behind a fetch"
            );
        })
        .unwrap();
    frame(cx, handle);
    handle
        .update(cx, |shell, _, _| {
            assert!(shell.panes[0].as_mut().unwrap().pane.begin(
                "/files".into(),
                crate::pane::swap::Options::default(),
                0,
            ));
        })
        .unwrap();
    for _ in 0..3 {
        frame(cx, handle);
    }
    handle
        .update(cx, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            if shell.view == View::List {
                let list = host.list.borrow();
                let loading: Vec<usize> = list
                    .visible()
                    .iter()
                    .filter(|row| row.prepared.is_none())
                    .map(|row| row.index)
                    .collect();
                assert!(
                    loading.is_empty(),
                    "the List landed while the reload was out and drew {} loading rows ({:?}..{:?}) at cursor {} over held {}+{}, busy {}",
                    loading.len(),
                    loading.first(),
                    loading.last(),
                    host.pane.cursor,
                    host.pane.held.start,
                    host.pane.held.rows.len(),
                    host.pane.busy()
                );
            }
        })
        .unwrap();
}
