//! Pointer selection must edit only the dragged part of an inline filename.
use super::{close, draw, editing, ready, sent};
use crate::{state::Shell, window::startup::View};
use gpui::{Entity, EntityInputHandler, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use std::time::Duration;

fn drag(shell: &Entity<Shell>, cx: &mut VisualTestContext, from: usize, to: usize) {
    let (start, end) = cx.update(|window, cx| {
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
    });
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
