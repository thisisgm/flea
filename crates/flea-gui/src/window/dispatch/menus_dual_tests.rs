//! A right click in the unfocused Dual pane leaves the focus where it was, as Tap.js tappedMenu does, and its menu acts on its own pane.
use super::menus_tests::{actions, answer, choose, home, right_click, settle, validated};
use super::test_shell::{asked_on, land, land_on, open_root as open};
use crate::{state::Shell, window::startup::View};
use gpui::{px, Entity, VisualTestContext};

// A point inside a pane's drawn rows: on a row's middle, or below the last row for the folder's own menu.
pub(super) fn spot(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    row: Option<usize>,
) -> (f32, f32) {
    shell.read_with(cx, |shell, _| {
        let input = &shell.panes[side].as_ref().unwrap().input;
        let bounds = input.frame.expect("the pane drew its rows").bounds;
        let x = bounds.left() + px(40.);
        let top = f32::from(bounds.top());
        let ys: Vec<f32> = (0..f32::from(bounds.size.height) as usize)
            .map(|dy| top + dy as f32)
            .filter(|&y| input.hit(gpui::point(x, px(y))) == row)
            .collect();
        let y = match row {
            Some(_) => (ys.first().expect("the row is drawn") + ys.last().unwrap()) / 2.,
            None => *ys.last().expect("the folder has space below its rows"),
        };
        (f32::from(x), y)
    })
}

fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> (usize, Vec<usize>, Vec<usize>) {
    shell.read_with(cx, |shell, _| {
        let selected = |side: usize| shell.panes[side].as_ref().unwrap().selected.clone();
        (shell.focus_side, selected(0), selected(1))
    })
}

#[gpui::test]
fn a_right_click_in_the_unfocused_pane_keeps_the_focus_and_its_menu_acts_on_that_pane(
    cx: &mut gpui::TestAppContext,
) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx)));
    let right = asked_on(&shell, cx, 1);
    land_on(&shell, cx, 1, &right, home(), 4, 1);
    shell.update(cx, |shell, _| {
        shell.panes[0].as_mut().unwrap().selected = vec![1]
    });
    settle(cx);
    assert_eq!(
        state(&shell, cx),
        (0, vec![1], vec![]),
        "the left pane holds the focus and one mark"
    );
    let (x, y) = spot(&shell, cx, 1, Some(2));
    right_click(cx, x, y);
    let side = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().map(|c| c.side));
    assert_eq!(
        side,
        Some(1),
        "the menu opens on the pane under the pointer"
    );
    assert_eq!(
        state(&shell, cx),
        (0, vec![1], vec![]),
        "a right click moves neither the focus nor a mark"
    );
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.menu.captured.is_none()),
        "Escape closes the menu"
    );
    assert_eq!(
        state(&shell, cx),
        (0, vec![1], vec![]),
        "Escape leaves the focus where it was"
    );
    let (x, y) = spot(&shell, cx, 1, None);
    right_click(cx, x, y);
    assert!(
        actions(&shell, cx).contains(&"selectAll".to_owned()),
        "{:?}",
        actions(&shell, cx)
    );
    choose(&shell, cx, "selectAll");
    assert_eq!(
        state(&shell, cx),
        (0, vec![1], vec![0, 1, 2, 3]),
        "Select All marks the captured pane only"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn an_unread_favourite_in_the_unfocused_pane_says_so_in_that_pane(cx: &mut gpui::TestAppContext) {
    let (shell, cx) = open(cx, "default");
    land(&shell, cx, "/files", home(), 1);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(View::Dual, window, cx)));
    let right = asked_on(&shell, cx, 1);
    land_on(&shell, cx, 1, &right, home(), 4, 1);
    settle(cx);
    let (x, y) = spot(&shell, cx, 1, Some(0));
    // The snapshot answers before the loading mark is due, so the menu draws at once.
    let position = gpui::point(px(x), px(y));
    let modifiers = gpui::Modifiers::none();
    let button = gpui::MouseButton::Right;
    cx.simulate_event(gpui::MouseDownEvent {
        position,
        button,
        modifiers,
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        position,
        button,
        modifiers,
        click_count: 1,
    });
    answer(&shell, cx, &[0o040755]);
    choose(&shell, cx, "addFavourite");
    validated(&shell, cx, "addFavourite", &[]);
    let said = shell.read_with(cx, |shell, _| {
        let status = |side: usize| shell.panes[side].as_ref().unwrap().status.to_string();
        let unread = shell.locale.catalog.tr("menu.refusal.favourite_unread");
        (shell.focus_side, status(0) == unread, status(1) == unread)
    });
    assert_eq!(
        said,
        (0, false, true),
        "the refusal lands in the clicked pane and the focus stays"
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
