//! A tracking marquee consumes keys before the window's settings, search and card shortcuts.
use super::{
    pb_grid_tests::switch,
    rename_tests::{close, draw, ready},
};
use crate::window::startup::View;
use gpui::{point, px, Modifiers, MouseButton, TestAppContext};

#[gpui::test]
fn pb_grid_tracking_band_swallows_early_shortcuts_and_escape_rolls_back(cx: &mut TestAppContext) {
    let (shell, cx, _lines) = ready(cx, "default");
    switch(&shell, cx, View::List);
    let (ground, end) = shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        let frame = host.input.frame.unwrap();
        (
            point(
                frame.bounds.left() + px(10.),
                frame.bounds.top() + px(frame.row * 5.),
            ),
            point(
                frame.bounds.left() + px(100.),
                frame.bounds.top() + px(frame.row / 2.),
            ),
        )
    });
    let before = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().selected.clone()
    });
    cx.simulate_mouse_move(ground, None, Modifiers::none());
    cx.simulate_mouse_down(ground, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    draw(cx);
    shell.read_with(cx, |shell, _| {
        assert!(shell.panes[0].as_ref().unwrap().input.band.is_some())
    });
    for key in ["ctrl-,", "/", "ctrl-f", "f2", "ctrl-q"] {
        cx.simulate_keystrokes(key);
        shell.read_with(cx, |shell, _| {
            assert!(
                !shell.settings.opened,
                "{key} cannot open Settings during a band"
            );
            assert!(
                shell.rename.edit.is_none(),
                "{key} cannot rename during a band"
            );
            assert!(
                shell.panes[0].as_ref().unwrap().input.band.is_some(),
                "{key} cannot dismiss the band"
            );
            assert_eq!(
                shell.lifecycle.phase(),
                crate::window::lifecycle::Phase::Open
            );
        });
    }
    cx.simulate_keystrokes("escape");
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.band.is_none());
        assert_eq!(host.selected, before);
    });
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    close(&shell, cx);
}
