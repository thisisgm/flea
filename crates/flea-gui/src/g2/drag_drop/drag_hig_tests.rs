//! The HIG drag-cut-move board requires an arrow for a held file move.
use super::mounted_tests::{centre, frame, open, Root};
use gpui::{point, px, CursorStyle, Modifiers, MouseButton, TestAppContext, VisualTestContext};

#[gpui::test]
fn sol_drag_hig_rc2_held_move_owns_the_standard_arrow(cx: &mut TestAppContext) {
    let root = Root::new("hig-arrow");
    let (handle, mut requests) = open(cx, root.src());
    frame(cx, handle);
    let from = centre(cx, handle, 1);
    let to = centre(cx, handle, 0);
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    visual.simulate_mouse_move(
        point(from.x + px(12.), from.y),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    for _ in 0..2 {
        visual.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
        visual.run_until_parked();
    }
    handle
        .update(&mut visual, |shell, window, cx| {
            let states = shell.list_states(0, window, cx);
            assert_eq!(states.drop_label.as_deref(), Some("move here"));
            assert_eq!(
                cx.active_drag_cursor_style(),
                Some(CursorStyle::Arrow),
                "the file drag must explicitly own the HIG arrow"
            );
            assert!(shell.escape_file_drag(window, cx));
            assert!(!cx.has_active_drag());
        })
        .unwrap();
    assert!(
        !std::iter::from_fn(&mut requests)
            .any(|line| line.contains("collisions") || line.contains("transfer")),
        "holding and cancelling must not send a write"
    );
}
