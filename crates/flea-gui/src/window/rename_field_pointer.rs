//! Pointer capture belongs to an inline editor only after a press inside its field.
use super::Shell;
use gpui::{
    Bounds, DispatchPhase, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Window,
};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

/// A double-click takes the word of the character at byte `at` as a text input does, and a press past the end takes the last one.
fn word(text: &str, at: usize) -> Range<usize> {
    let at = at.min(text.len().saturating_sub(1));
    text.split_word_bound_indices()
        .map(|(start, word)| start..start + word.len())
        .find(|word| word.contains(&at))
        .unwrap_or(at..at)
}

/// Captures filename selection drags and retires them on release, a menu, or an inactive editor.
pub(super) fn register(
    entity: Entity<Shell>,
    frame: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    window: &mut Window,
) {
    let down = entity.clone();
    // Tap.js: a press anywhere else commits the open rename first and then does its own work.
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture
            || !matches!(event.button, MouseButton::Left | MouseButton::Right)
        {
            return;
        }
        down.update(cx, |shell, cx| {
            if shell.menu.text.is_some() {
                return;
            }
            if event.button == MouseButton::Right {
                if frame.intersect(&clip).contains(&event.position) {
                    shell.rename_menu_open(event, window, cx);
                    cx.stop_propagation();
                }
                return;
            }
            let Some(edit) = shell.rename.edit.as_mut().filter(|edit| edit.entry.editing) else {
                return;
            };
            if !frame.intersect(&clip).contains(&event.position) {
                shell.rename_press_away(event.position, window, cx);
                return;
            }
            let x = event.position.x - edit.line.origin();
            let at = edit.line.line.index_for(x);
            edit.line.selecting = event.click_count == 1 && edit.pending.is_none();
            match event.click_count {
                1 if event.modifiers.shift => edit.entry.select_to(at),
                1 => edit.entry.move_to(at),
                2 => {
                    // The word is the glyph's under the pointer, not the nearer caret edge's.
                    let run = edit.line.line.run.as_ref();
                    let under = run.and_then(|run| run.line().index_for_x(x));
                    let word = word(&edit.entry.text, under.unwrap_or(edit.entry.text.len()));
                    edit.entry.move_to(word.start);
                    edit.entry.select_to(word.end);
                }
                _ => edit.entry.select_all(),
            }
            cx.stop_propagation();
            cx.notify();
        });
    });
    let moving = entity.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
        if phase != DispatchPhase::Capture {
            return;
        }
        moving.update(cx, |shell, cx| {
            let Some(edit) = shell.rename.edit.as_mut() else {
                return;
            };
            if !edit.line.selecting {
                return;
            }
            if event.pressed_button != Some(MouseButton::Left)
                || edit.pending.is_some()
                || !edit.entry.editing
            {
                edit.line.selecting = false;
                return;
            }
            let at = edit
                .line
                .line
                .index_for(event.position.x - edit.line.origin());
            edit.entry.select_to(at);
            cx.stop_propagation();
            cx.notify();
        });
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
        if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
            return;
        }
        entity.update(cx, |shell, cx| {
            if let Some(edit) = shell.rename.edit.as_mut() {
                if std::mem::take(&mut edit.line.selecting) {
                    cx.stop_propagation();
                }
            }
        });
    });
}
