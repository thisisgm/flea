//! A tracking marquee consumes keys before the window's settings, search and card shortcuts.
use super::{
    pb_grid_tests::{at, switch},
    rename_tests::{close, draw, ready},
};
use crate::window::startup::View;
use gpui::{point, px, Modifiers, MouseButton, TestAppContext};

#[gpui::test]
fn pb_grid_tracking_band_swallows_early_shortcuts_and_escape_rolls_back(cx: &mut TestAppContext) {
    band_swallows_keys(cx, View::List);
}

#[gpui::test]
fn pb_grid_grid_band_swallows_early_shortcuts_and_escape_rolls_back(cx: &mut TestAppContext) {
    band_swallows_keys(cx, View::Grid);
}

fn band_swallows_keys(cx: &mut TestAppContext, view: View) {
    let (shell, cx, _lines) = ready(cx, "default");
    switch(&shell, cx, view);
    let (ground, end) = if view == View::Grid {
        // Blank ground under the three tiles, dragged up onto the first tile.
        let ground = shell.read_with(cx, |shell, _| {
            let grid = shell.panes[0].as_ref().unwrap().grid.as_ref().unwrap();
            let bounds = grid.state.borrow().bounds;
            bounds.origin + point(px(30.), bounds.size.height - px(60.))
        });
        (ground, at(&shell, cx, 0, false))
    } else {
        shell.read_with(cx, |shell, _| {
            let frame = shell.panes[0].as_ref().unwrap().input.frame.unwrap();
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
        })
    };
    // A mark the band replaces, so a rollback differs from a cleared selection.
    let before = vec![1];
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = before.clone()
    });
    cx.simulate_mouse_move(ground, None, Modifiers::none());
    cx.simulate_mouse_down(ground, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.input.band.is_some());
        assert!(
            host.selected.contains(&0) && host.selected != before,
            "the band marks the first row it covers: {:?}",
            host.selected
        );
    });
    for key in ["ctrl-,", "/", "ctrl-f", "f2", "ctrl-q"] {
        cx.simulate_keystrokes(key);
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                !shell.settings.opened,
                "{key} cannot open Settings during a band"
            );
            assert!(
                shell.rename.edit.is_none(),
                "{key} cannot rename during a band"
            );
            assert!(
                !host.filter.owner.typing(),
                "{key} cannot open the filter during a band"
            );
            assert!(
                !host.search.owner.active(),
                "{key} cannot start a search during a band"
            );
            assert!(host.input.band.is_some(), "{key} cannot dismiss the band");
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
    // The button stays held after Escape: the drag goes back to the ground and out over the rows again.
    for at in [ground, end] {
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
        draw(cx);
        shell.read_with(cx, |shell, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                host.input.band.is_none(),
                "a held move after Escape draws no band"
            );
            assert_eq!(
                host.selected, before,
                "a held move after Escape keeps the rollback"
            );
        });
    }
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    draw(cx);
    shell.read_with(cx, |shell, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.input.band.is_none(),
            "the release after Escape draws no band"
        );
        assert_eq!(
            host.selected, before,
            "the release over the rows keeps the rollback"
        );
    });
    cx.simulate_keystrokes("/");
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.panes[0].as_ref().unwrap().filter.owner.typing(),
            "with the band gone, / opens the filter"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("ctrl-f");
    shell.read_with(cx, |shell, _| {
        assert!(
            shell.panes[0].as_ref().unwrap().search.owner.active(),
            "with the band gone, ctrl-f starts a search"
        );
    });
    close(&shell, cx);
}
