//! Exercise the mounted window, real row painter, event capture and one rest timer.
use super::columns_tests::host;
use crate::{
    name_tip::tests::{receipt, Probe},
    window::startup::View,
};
use gpui::{point, px, Modifiers, VisualTestContext};
use std::time::Duration;

const LONG: &str =
    "riverside-community-garden-committee-meeting-notes-2025-06-02-spring-review-final-approved.md";

fn wait(cx: &mut VisualTestContext, handle: gpui::WindowHandle<crate::state::Shell>) {
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    host::frame(cx, handle);
}

#[test]
fn name_tip_shapes_only_one_hovered_name_and_reuses_one_tip() {
    let mut cx = host::context();
    cx.update(|cx| cx.set_global(Probe::default()));
    let (handle, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 1100., 3);
    host::change(&mut cx, handle, |shell, window, cx| {
        host::peek(shell, "/srv/a/b", &[(LONG, false), ("second-community-garden-committee-meeting-notes-2025-06-02-spring-review-final-approved.md", false)], false, window, cx);
        host::settle(shell, window, cx);
        window.activate_window();
    });
    cx.run_until_parked();
    host::frame(&mut cx, handle);
    let at = handle
        .read_with(&cx, |shell, _| {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            let paint = columns
                .painters
                .get(crate::columns::geometry::Slot::Parent)
                .unwrap()
                .borrow();
            let bounds = paint.drawn.as_ref().unwrap().bounds;
            point(bounds.left() + px(80.), bounds.top() + px(15.))
        })
        .unwrap();
    let mut cx = VisualTestContext::from_window(handle.into(), &cx);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    host::frame(&mut cx, handle);
    cx.update(|_, cx| crate::name_tip::tests::assert_foreground(cx));
    let diagnostic = handle
        .update(&mut cx, |shell, window, cx| {
            crate::name_tip::tests::diagnostic(shell, window, cx)
        })
        .unwrap();
    assert_eq!(
        cx.update(|_, cx| receipt(cx)),
        (false, 0, 0, true),
        "{diagnostic}"
    );
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    cx.simulate_mouse_move(at + point(px(1.), px(0.)), None, Modifiers::none());
    host::frame(&mut cx, handle);
    cx.executor().advance_clock(Duration::from_millis(499));
    cx.run_until_parked();
    host::frame(&mut cx, handle);
    assert_eq!(cx.update(|_, cx| receipt(cx)), (false, 0, 0, true));
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    host::frame(&mut cx, handle);
    assert_eq!(cx.update(|_, cx| receipt(cx)), (true, 1, 1, true));
    cx.simulate_mouse_move(at + point(px(3.), px(0.)), None, Modifiers::none());
    host::frame(&mut cx, handle);
    assert_eq!(
        cx.update(|_, cx| receipt(cx)),
        (true, 1, 1, true),
        "same name remains shown"
    );
    cx.simulate_mouse_move(at + point(px(2.), px(31.)), None, Modifiers::none());
    host::frame(&mut cx, handle);
    assert_eq!(
        cx.update(|_, cx| receipt(cx)),
        (false, 1, 1, true),
        "new row starts its own rest"
    );
    cx.executor().advance_clock(Duration::from_millis(499));
    cx.run_until_parked();
    host::frame(&mut cx, handle);
    assert_eq!(cx.update(|_, cx| receipt(cx)), (false, 1, 1, true));
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    host::frame(&mut cx, handle);
    assert_eq!(cx.update(|_, cx| receipt(cx)), (true, 2, 2, true));
    for _ in 0..8 {
        host::frame(&mut cx, handle);
    }
    assert_eq!(cx.update(|_, cx| receipt(cx)), (true, 2, 2, true));
    for (rounding, radius) in [(0., 0.), (8., 4.), (12., 6.)] {
        assert_eq!(
            crate::name_tip::tests::plate_radii(&mut cx, rounding),
            vec![radius; 4],
            "painted plate at compositor rounding {rounding}"
        );
    }
    assert_eq!(cx.update(|_, cx| receipt(cx)), (true, 2, 2, true));
    cx.simulate_mouse_move(point(px(5.), px(5.)), None, Modifiers::none());
    host::frame(&mut cx, handle);
    assert_eq!(cx.update(|_, cx| receipt(cx)), (false, 2, 2, false));
    host::change(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::Grid, window, cx)
    });
    host::frame(&mut cx, handle);
    assert_eq!(cx.update(|_, cx| receipt(cx)), (false, 2, 2, false));
}

#[test]
fn name_tip_real_key_scroll_press_drag_and_menu_events_arbitrate() {
    for action in [
        "key", "modifier", "scroll", "press", "drag", "menu", "rename", "swap", "blur",
    ] {
        let mut cx = host::context();
        cx.update(|cx| cx.set_global(Probe::default()));
        let (handle, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 1100., 3);
        host::change(&mut cx, handle, |shell, window, cx| {
            host::peek(shell, "/srv/a/b", &[(LONG, false)], false, window, cx);
            host::settle(shell, window, cx);
            window.activate_window();
        });
        cx.run_until_parked();
        host::frame(&mut cx, handle);
        let at = handle
            .read_with(&cx, |shell, _| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let paint = columns
                    .painters
                    .get(crate::columns::geometry::Slot::Parent)
                    .unwrap()
                    .borrow();
                let bounds = paint.drawn.as_ref().unwrap().bounds;
                point(bounds.left() + px(80.), bounds.top() + px(15.))
            })
            .unwrap();
        let mut cx = VisualTestContext::from_window(handle.into(), &cx);
        cx.simulate_mouse_move(at, None, Modifiers::none());
        host::frame(&mut cx, handle);
        wait(&mut cx, handle);
        let diagnostic = handle
            .update(&mut cx, |shell, window, cx| {
                crate::name_tip::tests::diagnostic(shell, window, cx)
            })
            .unwrap();
        assert!(
            cx.update(|_, cx| receipt(cx).0),
            "{action}: positive control: {diagnostic}"
        );
        match action {
            "key" => cx.simulate_keystrokes("g"),
            "blur" => cx.deactivate_window(),
            "modifier" => cx.simulate_event(gpui::ModifiersChangedEvent {
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
                capslock: Default::default(),
            }),
            "scroll" => cx.simulate_event(gpui::ScrollWheelEvent {
                position: at,
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-31.))),
                ..Default::default()
            }),
            "press" => cx.simulate_mouse_down(at, gpui::MouseButton::Left, Modifiers::none()),
            "drag" => cx.simulate_mouse_move(
                at + point(px(10.), px(0.)),
                Some(gpui::MouseButton::Left),
                Modifiers::none(),
            ),
            "menu" => {
                host::change(&mut cx, handle, |shell, window, cx| {
                    shell.open_menu(0, None, (f32::from(at.x), f32::from(at.y)), window, cx);
                });
                handle
                    .read_with(&cx, |shell, _| assert!(shell.menu_opened()))
                    .unwrap();
                cx.simulate_mouse_move(at + point(px(1.), px(0.)), None, Modifiers::none());
            }
            "rename" => host::change(&mut cx, handle, |shell, window, cx| {
                assert!(shell.rename_open(0, 1, window, cx));
                assert!(shell.renaming_row(0).is_some());
            }),
            "swap" => host::change(&mut cx, handle, |shell, _, _| {
                let host = shell.panes[0].as_mut().unwrap();
                host.pane
                    .receive(flea::protocol::Reply::Listed {
                        path: "/srv/a/b/c".into(),
                        total: 1,
                    })
                    .unwrap();
                host.pane
                    .receive(flea::protocol::Reply::Rows {
                        start: 0,
                        listing: 31,
                        rows: vec![host::row("fresh.txt", false)],
                    })
                    .unwrap();
            }),
            _ => unreachable!(),
        }
        host::frame(&mut cx, handle);
        assert!(!cx.update(|_, cx| receipt(cx).0), "{action}: tip dismissed");
        wait(&mut cx, handle);
        assert!(
            !cx.update(|_, cx| receipt(cx).0),
            "{action}: stale rest cannot reopen tip"
        );
    }
}

#[test]
fn name_tip_fitting_rows_and_hidden_views_shape_nothing() {
    // The last case is the control: same parent row and point, only the name or the shown view changes.
    for (name, hidden, opened) in [
        ("fits.md", false, false),
        (LONG, true, false),
        (LONG, false, true),
    ] {
        let mut cx = host::context();
        cx.update(|cx| cx.set_global(Probe::default()));
        let (handle, _requests) = host::open_at(&mut cx, "/srv/a/b/c", 1100., 3);
        host::change(&mut cx, handle, |shell, window, cx| {
            host::peek(shell, "/srv/a/b", &[(name, false)], false, window, cx);
            host::settle(shell, window, cx);
            window.activate_window();
        });
        cx.run_until_parked();
        host::frame(&mut cx, handle);
        let at = handle
            .read_with(&cx, |shell, _| {
                let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
                let paint = columns
                    .painters
                    .get(crate::columns::geometry::Slot::Parent)
                    .unwrap()
                    .borrow();
                let bounds = paint.drawn.as_ref().unwrap().bounds;
                point(bounds.left() + px(80.), bounds.top() + px(15.))
            })
            .unwrap();
        if hidden {
            host::change(&mut cx, handle, |shell, window, cx| {
                shell.set_view(View::Grid, window, cx)
            });
            host::frame(&mut cx, handle);
        }
        let mut cx = VisualTestContext::from_window(handle.into(), &cx);
        cx.simulate_mouse_move(at, None, Modifiers::none());
        host::frame(&mut cx, handle);
        wait(&mut cx, handle);
        let expected = if opened {
            (true, 1, 1, true)
        } else {
            (false, 0, 0, false)
        };
        assert_eq!(
            cx.update(|_, cx| receipt(cx)),
            expected,
            "{name} hidden={hidden}"
        );
    }
}
