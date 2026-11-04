//! A Shift tap on empty ground keeps the marks, where a plain tap clears them.
use super::test_shell::{land, open_root_with, row};
use gpui::{point, px, Modifiers, TestAppContext};

#[gpui::test]
fn fix_band_shift_ground_tap_keeps_the_marks(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"list""#, None);
    let rows = (0..5).map(|i| row(&format!("item-{i:02}.txt"), false));
    land(&shell, cx, "/files", rows.collect(), 1);
    let frame = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().input.frame.unwrap()
    });
    let first = point(
        frame.bounds.left() + px(100.),
        frame.bounds.top() + px(frame.row / 2.),
    );
    let ground = point(
        frame.bounds.left() + px(100.),
        frame.bounds.bottom() - px(20.),
    );
    let shift = Modifiers {
        shift: true,
        ..Modifiers::none()
    };
    cx.simulate_mouse_move(first, None, Modifiers::none());
    cx.simulate_click(first, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, [0]);
    });
    cx.simulate_mouse_move(ground, None, shift);
    cx.simulate_click(ground, shift);
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.band.is_none(), "the tap ended its band");
        assert_eq!(host.selected, [0], "a Shift ground tap keeps the marks");
    });
    cx.simulate_mouse_move(ground, None, Modifiers::none());
    cx.simulate_click(ground, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.selected.is_empty(),
            "a plain tap on the same ground clears them"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
