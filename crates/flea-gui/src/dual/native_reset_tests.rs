//! The mounted divider consumes a genuine toolkit double click after resizing.
use crate::{backend::Backend, state::tests::shell_with, window::startup::View};
use gpui::{
    point, px, size, Modifiers, MouseButton, MouseDownEvent, TestAppContext, VisualTestContext,
};

#[gpui::test]
fn vf_look_divider_resets_on_a_genuine_double_click(cx: &mut TestAppContext) {
    let (backend, _) = Backend::test_channel();
    let handle = cx.open_window(size(px(1100.), px(700.)), |_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.view = View::Dual;
        shell
    });
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.run_until_parked();
    let from = point(px(646.), px(350.));
    let to = point(px(770.), px(350.));
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    handle
        .update(&mut visual, |shell, _, _| {
            assert!(shell.launch.settings.split > 0.6)
        })
        .unwrap();
    visual.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: to,
                modifiers: Modifiers::none(),
                click_count: 2,
                first_mouse: false,
            }),
            cx,
        );
    });
    visual.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    visual.run_until_parked();
    handle
        .update(&mut visual, |shell, _, _| {
            assert_eq!(shell.launch.settings.split, 0.5)
        })
        .unwrap();
}
