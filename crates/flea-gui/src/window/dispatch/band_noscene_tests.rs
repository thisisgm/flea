//! NOSCENE-34's band proof, kept beside band_tests for its open and drag helpers.
use super::*;

// Marquee.js additive: a Ctrl band keeps every mark made before it, row 4 inside the band included; a plain band replaces them.
#[gpui::test]
fn noscene_a_ctrl_band_adds_to_the_existing_marks(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx, "columns", 6);
    cx.simulate_keystrokes("escape v down down down down v");
    cx.run_until_parked();
    let (start, end) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, [0, 4], "v marks rows 0 and 4");
        let paint = host
            .columns
            .as_ref()
            .unwrap()
            .painters
            .get(Slot::Active)
            .unwrap()
            .borrow();
        let drawn = paint.drawn.as_ref().expect("active column is mounted");
        let row = host.pane.row_height();
        (
            point(
                drawn.bounds.left() + px(100.),
                drawn.bounds.bottom() - px(40.),
            ),
            point(
                drawn.bounds.left() + px(30.),
                drawn.bounds.top() + px(row * 3.5),
            ),
        )
    });
    let control = Modifiers {
        control: true,
        ..Default::default()
    };
    cx.simulate_mouse_move(start, None, control);
    cx.simulate_mouse_down(start, MouseButton::Left, control);
    cx.simulate_mouse_move(end, Some(MouseButton::Left), control);
    cx.run_until_parked();
    cx.simulate_mouse_up(end, MouseButton::Left, control);
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().selected,
            [0, 3, 4, 5],
            "the Ctrl band adds rows 3 and 5 and keeps the marked rows 0 and 4"
        );
    });
    drag(cx, start, end);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    shell.read_with(cx, |shell, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().selected,
            [3, 4, 5],
            "a plain band replaces the marks"
        );
    });
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
