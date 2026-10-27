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
