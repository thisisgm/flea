//! Modified wheels retain the listing's normal distance and phased touchpad policy.
use crate::{backend::Backend, state::tests::shell_with};
use gpui::{point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TestAppContext, TouchPhase};

#[gpui::test]
fn pb_scroll_list_control_wheel_scrolls_normally(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.run_until_parked();
    let at = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .input
            .frame
            .unwrap()
            .bounds
            .center()
    });
    for (control, expected) in [(false, 288.), (true, 576.)] {
        cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Lines(point(0., -3.)),
            modifiers: Modifiers {
                control,
                ..Modifiers::none()
            },
            ..Default::default()
        });
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.panes[0].as_ref().unwrap().pane.scroll_y, expected);
        });
    }
}

#[gpui::test]
fn pb_scroll_list_control_touchpad_uses_stroke_gain(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    cx.run_until_parked();
    let at = shell.read_with(cx, |shell, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .input
            .frame
            .unwrap()
            .bounds
            .center()
    });
    for (phase, y) in [(TouchPhase::Started, 0.), (TouchPhase::Moved, -24.)] {
        cx.simulate_event(ScrollWheelEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.), px(y))),
            modifiers: Modifiers {
                control: true,
                ..Modifiers::none()
            },
            touch_phase: phase,
        });
    }
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.scroll_y, 20.);
    });
}

// Scroll.js touchDistance: a touchpad pixel moves 2.5 pixels, a zero frame moves nothing.
#[test]
fn noscene_touch_distance_is_the_stroke_gain() {
    use crate::input::scroll::{touch_distance, TOUCH_GAIN};
    assert_eq!(TOUCH_GAIN, 2.5);
    assert_eq!(
        (touch_distance(8.), touch_distance(-4.), touch_distance(0.)),
        (20., -10., 0.)
    );
}

// FastScrollHandler.qml: each Dual pane takes its own touchpad stroke at the same gain as List.
#[gpui::test]
fn noscene_dual_panes_take_the_touchpad_stroke_gain(cx: &mut TestAppContext) {
    use crate::window::{pane::PaneHost, startup::View};
    use flea::protocol::{Reply, RowFacts};
    let (backend, _requests) = Backend::test_channel();
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(backend, cx);
        let mut right = PaneHost::new("/right".into(), None, 620., &shell.locale);
        right
            .pane
            .receive(Reply::Listed {
                path: "/right".into(),
                total: 1000,
            })
            .unwrap();
        right
            .pane
            .receive(Reply::Rows {
                start: 0,
                rows: vec![RowFacts::default(); 320],
                listing: 2,
            })
            .unwrap();
        shell.panes[1] = Some(right);
        shell.view = View::Dual;
        shell
    });
    cx.run_until_parked();
    for side in [1, 0] {
        let at = shell.read_with(cx, |shell, _| {
            shell.panes[side]
                .as_ref()
                .unwrap()
                .input
                .frame
                .unwrap()
                .bounds
                .center()
        });
        for (phase, y) in [(TouchPhase::Started, 0.), (TouchPhase::Moved, -24.)] {
            cx.simulate_event(ScrollWheelEvent {
                position: at,
                delta: ScrollDelta::Pixels(point(px(0.), px(y))),
                modifiers: Modifiers::none(),
                touch_phase: phase,
            });
        }
        shell.read_with(cx, |shell, _| {
            assert_eq!(shell.panes[side].as_ref().unwrap().pane.scroll_y, 20.);
        });
    }
}
