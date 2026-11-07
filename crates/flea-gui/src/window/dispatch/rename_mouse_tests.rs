//! Pointer selection must edit only the dragged part of an inline filename.
#[path = "rename_mouse_more_tests.rs"]
mod more;
use super::{close, draw, editing, ready, ready_root, receive, sent};
use crate::{backend::Event as BackendEvent, state::Shell, window::startup::View};
use gpui::{Entity, EntityInputHandler, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::time::Duration;

/// Finds two byte offsets through the field's shaped UTF-16 platform geometry.
fn points(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    from: usize,
    to: usize,
) -> (gpui::Point<gpui::Pixels>, gpui::Point<gpui::Pixels>) {
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let edit = shell.rename.edit.as_ref().unwrap();
            let frame = edit.line.frame.unwrap();
            let (from, to) = (
                edit.entry.offset_to_utf16(from),
                edit.entry.offset_to_utf16(to),
            );
            let mut at = |offset| {
                shell
                    .bounds_for_range(offset..offset, frame, window, cx)
                    .unwrap()
                    .center()
            };
            (at(from), at(to))
        })
    })
}

/// Delivers a complete selection gesture and verifies mouse-up releases its anchor.
fn drag(shell: &Entity<Shell>, cx: &mut VisualTestContext, from: usize, to: usize) {
    let (start, end) = points(shell, cx, from, to);
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    draw(cx);
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    draw(cx);
    let selected = editing(shell, cx).unwrap().2;
    // A later gesture must not inherit the field's old selection anchor.
    cx.simulate_mouse_move(start, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(editing(shell, cx).unwrap().2, selected);
}

/// Backspace removes only the dragged range without submitting a rename.
#[gpui::test]
fn dragging_in_the_rename_field_selects_only_that_text(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    drag(&shell, cx, 1, 4);
    assert_eq!(editing(&shell, cx).unwrap().2, "ote");
    cx.simulate_keystrokes("backspace");
    assert_eq!(editing(&shell, cx).unwrap().1, "ns.txt");
    assert!(
        sent(&mut lines).is_empty(),
        "selecting text must not rename the file"
    );
    close(&shell, cx);
}

/// Each listing layout keeps the same selection anchor in either drag direction.
#[gpui::test]
fn rename_mouse_selection_works_in_both_directions_in_every_view(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns, View::Grid, View::Dual] {
        for (from, to) in [(1, 4), (4, 1)] {
            let (shell, cx, _) = ready(cx, "default");
            cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(view, window, cx)));
            for _ in 0..4 {
                cx.executor().advance_clock(Duration::from_millis(400));
                draw(cx);
            }
            cx.simulate_keystrokes("f2");
            draw(cx);
            drag(&shell, cx, from, to);
            assert_eq!(
                editing(&shell, cx).unwrap().2,
                "ote",
                "{view:?}, {from} → {to}"
            );
            cx.simulate_keystrokes("backspace");
            assert_eq!(editing(&shell, cx).unwrap().1, "ns.txt");
            close(&shell, cx);
        }
    }
}

/// Pointer offsets preserve UTF-8 filenames when the selected word is replaced.
#[gpui::test]
fn rename_mouse_selection_preserves_unicode_and_replaces_the_selected_word(
    cx: &mut TestAppContext,
) {
    let (shell, cx, _) = ready(cx, "default");
    cx.simulate_keystrokes("f2");
    cx.simulate_input("моя деталь");
    draw(cx);
    drag(&shell, cx, "моя ".len(), "моя деталь".len());
    assert_eq!(editing(&shell, cx).unwrap().2, "деталь");
    cx.simulate_input("модель");
    assert_eq!(editing(&shell, cx).unwrap().1, "моя модель.txt");
    close(&shell, cx);
}

/// A context menu must freeze the selection even if the original left button stays down.
#[gpui::test]
fn rename_menu_retires_a_held_filename_selection(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready_root(cx, "default");
    cx.simulate_keystrokes("f2");
    draw(cx);
    let (start, end) = points(&shell, cx, 1, 4);
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    draw(cx);
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "ote");
    cx.simulate_mouse_down(end, MouseButton::Right, Modifiers::default());
    draw(cx);
    assert!(shell.read_with(cx, |shell, _| shell.menu.text.is_some()));
    cx.simulate_mouse_move(start, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(
        editing(&shell, cx).unwrap().2,
        "ote",
        "the menu freezes its Cut/Copy selection"
    );
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.retire_menu(window, cx)));
    draw(cx);
    cx.simulate_mouse_move(start, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(
        editing(&shell, cx).unwrap().2,
        "ote",
        "closing the menu must not revive the gesture"
    );
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    assert!(sent(&mut lines).is_empty());
    close(&shell, cx);
}

/// Opening the menu ends the gesture even if no move is delivered while the menu is open.
#[gpui::test]
fn rename_menu_escape_without_motion_ends_selection(cx: &mut TestAppContext) {
    for release_left_in_menu in [false, true] {
        let (shell, cx, mut lines) = ready_root(cx, "default");
        cx.simulate_keystrokes("f2");
        draw(cx);
        let (start, end) = points(&shell, cx, 1, 4);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        draw(cx);
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        draw(cx);
        assert_eq!(editing(&shell, cx).unwrap().2, "ote");

        cx.simulate_mouse_down(end, MouseButton::Right, Modifiers::default());
        draw(cx);
        cx.simulate_mouse_up(end, MouseButton::Right, Modifiers::default());
        assert!(shell.read_with(cx, |shell, _| shell.menu.text.is_some()));
        if release_left_in_menu {
            cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        }
        cx.simulate_keystrokes("escape");
        draw(cx);
        assert!(shell.read_with(cx, |shell, _| shell.menu.text.is_none()));
        let held = (!release_left_in_menu).then_some(MouseButton::Left);
        cx.simulate_mouse_move(start, held, Modifiers::default());
        draw(cx);
        assert_eq!(
            editing(&shell, cx).unwrap().2,
            "ote",
            "Escape must not revive the drag"
        );
        if !release_left_in_menu {
            cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
        }
        drag(&shell, cx, 2, 5);
        assert_eq!(editing(&shell, cx).unwrap().2, "tes");
        assert!(sent(&mut lines).is_empty());
        close(&shell, cx);
    }
}

/// A backend refusal received before the next move must not revive a submitted drag.
#[gpui::test]
fn rename_refusal_without_motion_does_not_revive_selection(cx: &mut TestAppContext) {
    let (shell, cx, mut lines) = ready_root(cx, "default");
    cx.simulate_keystrokes("f2");
    cx.simulate_input("alpha");
    draw(cx);
    let (start, end) = points(&shell, cx, 1, 4);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    draw(cx);
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "lph");

    cx.simulate_keystrokes("enter");
    assert_eq!(sent(&mut lines).len(), 1);
    receive(
        &shell,
        cx,
        BackendEvent::Refused {
            operation: "rename".into(),
            path: "/files/notes.txt".into(),
            message: "file exists".into(),
            mode: 0,
        },
    );
    draw(cx);
    cx.simulate_mouse_move(start, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "lph");
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    assert!(sent(&mut lines).is_empty());
    close(&shell, cx);
}

/// Losing window activation ends the drag even when the release goes to another window.
#[gpui::test]
fn rename_window_blur_ends_selection_drag(cx: &mut TestAppContext) {
    let (shell, cx, _) = ready_root(cx, "default");
    cx.update(|window, _| window.activate_window());
    cx.simulate_keystrokes("f2");
    draw(cx);
    let (start, end) = points(&shell, cx, 1, 4);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    draw(cx);
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "ote");
    cx.deactivate_window();
    cx.run_until_parked();
    assert!(!cx.update(|window, _| window.is_window_active()));
    cx.update(|window, _| window.activate_window());
    draw(cx);
    cx.simulate_mouse_move(start, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "ote");
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::default());
    close(&shell, cx);
}
