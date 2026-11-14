//! Grid pointer releases leave the parked List axis alone.
use super::super::{
    rename_tests::{close, draw, ready_root},
    test_shell::{land, row},
};
use crate::window::{startup::View, u07_tests::read};
use gpui::{point, px, Modifiers, MouseButton, TestAppContext};
use std::time::Duration;

fn click_after_home(cx: &mut TestAppContext, already_grid: bool) {
    let (shell, cx, _lines) = ready_root(cx, "default");
    let rows = (0..40)
        .map(|n| row(&format!("f{n:04}.txt"), false))
        .collect();
    land(&shell, cx, "/files", rows, 2);
    if already_grid {
        cx.update(|window, cx| {
            shell.update(cx, |shell, cx| shell.set_view(View::Grid, window, cx))
        });
        draw(cx);
    }
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let host = shell.panes[0].as_mut().unwrap();
            host.pane.cursor = 39;
            host.selected = vec![39];
            host.pane.reveal_cursor();
            if !already_grid {
                shell.set_view(View::Grid, window, cx);
            }
        })
    });
    draw(cx);
    let parked = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.scroll_y
    });
    assert!(parked > 0.);
    cx.simulate_keystrokes("escape home");
    draw(cx);
    assert_eq!(read(&shell, cx, "cursor", &[]), "0");
    assert_eq!(read(&shell, cx, "viewContentY", &[]), "0");
    let centre = read(&shell, cx, "rowCentre", &["0"]);
    let at: Vec<f32> = centre
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    let at = point(px(at[0]), px(at[1]));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    for frame in 0..=10 {
        if frame > 0 {
            cx.executor().advance_clock(Duration::from_millis(100));
        }
        draw(cx);
        assert_eq!(read(&shell, cx, "cursor", &[]), "0", "frame {frame}");
        assert_eq!(
            read(&shell, cx, "selectedIndices", &[]),
            "0",
            "frame {frame}"
        );
        assert_eq!(read(&shell, cx, "viewContentY", &[]), "0", "frame {frame}");
        assert_eq!(
            shell.read_with(cx, |shell, _| shell.panes[0]
                .as_ref()
                .unwrap()
                .pane
                .scroll_y),
            parked,
            "frame {frame}: Grid must not write the parked List scroll"
        );
    }
    close(&shell, cx);
}

#[gpui::test]
fn grid_release_after_home_keeps_cursor_selection_and_scroll(cx: &mut TestAppContext) {
    click_after_home(cx, true);
}

#[gpui::test]
fn grid_release_after_switch_from_deep_list_keeps_cursor_selection_and_scroll(
    cx: &mut TestAppContext,
) {
    click_after_home(cx, false);
}
