//! A plain click, a shift-click, double and triple clicks, a drag past the frame, and wide or right-to-left names.
use super::super::{close, draw, editing, ready, sent};
use super::{drag, points};
use crate::{state::Shell, window::startup::View};
use gpui::{px, Entity, Modifiers, MouseButton, MouseDownEvent, TestAppContext, VisualTestContext};
use std::time::Duration;
use unicode_segmentation::UnicodeSegmentation;

/// Lists the fixture in one view and opens F2 on notes.txt.
fn opened(
    cx: &mut TestAppContext,
    view: View,
) -> (Entity<Shell>, &mut VisualTestContext, super::super::Lines) {
    let (shell, cx, lines) = ready(cx, "default");
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.set_view(view, window, cx)));
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(400));
        draw(cx);
    }
    cx.simulate_keystrokes("f2");
    draw(cx);
    (shell, cx, lines)
}

/// No backend request names a rename: the gestures only select.
fn renamed(lines: &mut super::super::Lines) -> bool {
    sent(lines)
        .iter()
        .any(|line| line.contains(r#""c":"rename""#))
}

fn cursor(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> usize {
    shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().entry.cursor()
    })
}

/// One press of the given count at a byte offset, released where it landed.
fn press(shell: &Entity<Shell>, cx: &mut VisualTestContext, at: usize, count: usize) {
    let (point, _) = points(shell, cx, at, at);
    cx.simulate_event(MouseDownEvent {
        position: point,
        button: MouseButton::Left,
        click_count: count,
        ..Default::default()
    });
    draw(cx);
    cx.simulate_mouse_up(point, MouseButton::Left, Modifiers::default());
    draw(cx);
}

/// A click with no drag places the caret, and a hover with no button held selects nothing.
#[gpui::test]
fn rename_click_without_drag_places_the_caret_in_every_view(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns, View::Grid] {
        let (shell, cx, mut lines) = opened(cx, view);
        assert_eq!(editing(&shell, cx).unwrap().2, "notes", "{view:?}");
        press(&shell, cx, 3, 1);
        assert_eq!(editing(&shell, cx).unwrap().2, "", "{view:?}");
        assert_eq!(cursor(&shell, cx), 3, "{view:?}");
        let (_, end) = points(&shell, cx, 3, 7);
        cx.simulate_mouse_move(end, None, Modifiers::default());
        draw(cx);
        assert_eq!(editing(&shell, cx).unwrap().2, "", "{view:?} hover");
        assert_eq!(cursor(&shell, cx), 3, "{view:?} hover");
        assert!(!renamed(&mut lines), "{view:?}");
        close(&shell, cx);
    }
}

/// A shift-click extends a dragged selection from its anchor, and a held shift-drag keeps extending it.
#[gpui::test]
fn rename_shift_click_extends_a_dragged_selection(cx: &mut TestAppContext) {
    let (shell, cx, _) = opened(cx, View::List);
    drag(&shell, cx, 1, 4);
    assert_eq!(editing(&shell, cx).unwrap().2, "ote");
    let (at, back) = points(&shell, cx, 7, 2);
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::shift());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "otes.t");
    cx.simulate_mouse_move(back, Some(MouseButton::Left), Modifiers::shift());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "o");
    cx.simulate_mouse_up(back, MouseButton::Left, Modifiers::shift());
    draw(cx);
    close(&shell, cx);
}

/// A double-click takes the word under the pointer and a triple-click the whole name, the same in every view.
#[gpui::test]
fn rename_double_click_takes_a_word_and_triple_click_the_name(cx: &mut TestAppContext) {
    for view in [View::List, View::Columns, View::Grid] {
        let (shell, cx, mut lines) = opened(cx, view);
        cx.simulate_input("my old notes");
        draw(cx);
        press(&shell, cx, 4, 1);
        press(&shell, cx, 4, 2);
        assert_eq!(editing(&shell, cx).unwrap().2, "old", "{view:?}");
        // The double-click's release ends the press: a later held move changes nothing.
        let (end, _) = points(&shell, cx, 10, 10);
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        draw(cx);
        assert_eq!(editing(&shell, cx).unwrap().2, "old", "{view:?}");
        press(&shell, cx, 4, 3);
        assert_eq!(
            editing(&shell, cx).unwrap().2,
            "my old notes.txt",
            "{view:?}"
        );
        press(&shell, cx, 4, 2);
        cx.simulate_input("new");
        assert_eq!(
            editing(&shell, cx).unwrap().1,
            "my new notes.txt",
            "{view:?}"
        );
        assert!(!renamed(&mut lines), "{view:?}");
        close(&shell, cx);
    }
}

/// A double-click on the right half of a word's last letter still takes that word, not the separator after it.
#[gpui::test]
fn rename_double_click_on_a_last_letter_takes_its_word(cx: &mut TestAppContext) {
    let (shell, cx, _) = opened(cx, View::List);
    cx.simulate_input("my old notes");
    draw(cx);
    let (left, right) = points(&shell, cx, 1, 2);
    let at = gpui::point(left.x + (right.x - left.x) * 0.75, left.y);
    for count in [1, 2] {
        cx.simulate_event(MouseDownEvent {
            position: at,
            button: MouseButton::Left,
            click_count: count,
            ..Default::default()
        });
        draw(cx);
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::default());
        draw(cx);
    }
    assert_eq!(editing(&shell, cx).unwrap().2, "my");
    close(&shell, cx);
}

/// A drag that leaves the frame on either side stops at that end of the name.
#[gpui::test]
fn rename_drag_past_the_frame_stops_at_the_name_ends(cx: &mut TestAppContext) {
    let (shell, cx, _) = opened(cx, View::List);
    let frame = shell.read_with(cx, |shell, _| {
        shell.rename.edit.as_ref().unwrap().line.frame.unwrap()
    });
    let (start, _) = points(&shell, cx, 1, 1);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    draw(cx);
    let right = gpui::point(frame.right() + px(200.), start.y + px(30.));
    cx.simulate_mouse_move(right, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "otes.txt");
    let left = gpui::point(frame.left() - px(200.), start.y - px(30.));
    cx.simulate_mouse_move(left, Some(MouseButton::Left), Modifiers::default());
    draw(cx);
    assert_eq!(editing(&shell, cx).unwrap().2, "n");
    cx.simulate_mouse_up(left, MouseButton::Left, Modifiers::default());
    draw(cx);
    close(&shell, cx);
}

/// Drags across wide, combined and right-to-left names land on character boundaries and never panic.
#[gpui::test]
fn rename_drag_over_wide_and_right_to_left_names_stays_on_boundaries(cx: &mut TestAppContext) {
    let (shell, cx, _) = opened(cx, View::List);
    for name in ["文件名称", "a👍🏽b", "שלום עולם", "abc مرحبا"] {
        cx.simulate_keystrokes("ctrl-a");
        cx.simulate_input(name);
        draw(cx);
        let text = editing(&shell, cx).unwrap().1;
        let stops: Vec<usize> = name
            .grapheme_indices(true)
            .map(|(at, _)| at)
            .chain([name.len()])
            .collect();
        let bidi = name.chars().any(|c| matches!(c, '\u{590}'..='\u{8ff}'));
        for &from in &stops {
            for &to in &stops {
                drag(&shell, cx, from, to);
                let (selected, edit) = shell.read_with(cx, |shell, _| {
                    let entry = &shell.rename.edit.as_ref().unwrap().entry;
                    (entry.selected.clone(), entry.text.clone())
                });
                assert_eq!(edit, text, "a drag never edits");
                assert!(
                    text.is_char_boundary(selected.start) && text.is_char_boundary(selected.end),
                    "{name:?} {from} -> {to}: {selected:?}"
                );
                if !bidi {
                    assert_eq!(
                        selected,
                        from.min(to)..from.max(to),
                        "{name:?} {from} -> {to}"
                    );
                }
            }
        }
    }
    close(&shell, cx);
}
