//! Pointer capture belongs to an inline editor only after a press inside its field.
use super::Shell;
use gpui::{
    Bounds, DispatchPhase, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Window,
};

/// Captures filename selection drags and retires them on release, a menu, or an inactive editor.
pub(super) fn register(
    entity: Entity<Shell>,
    frame: Bounds<Pixels>,
    clip: Bounds<Pixels>,
    window: &mut Window,
) {
    let down = entity.clone();
    // Tap.js commits on release, before that tap selects its row, so a fast reply cannot replace the pressed listing.
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
            edit.line.end_selection_drag();
            if !frame.intersect(&clip).contains(&event.position) {
                edit.line.pressed_away = edit.pending.is_none().then_some(event.position);
                return;
            }
            let x = event.position.x - edit.line.origin();
            let at = edit.line.line.index_for(x);
            edit.line.selecting = event.click_count == 1 && edit.pending.is_none();
            match event.click_count {
                1 if event.modifiers.shift => edit.entry.select_to(at),
                1 => edit.entry.move_to(at),
                2 => edit.entry.select_word_under(&edit.line.line, x),
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
            if event.pressed_button != Some(MouseButton::Left) {
                edit.line.pressed_away = None;
            }
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
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
            return;
        }
        entity.update(cx, |shell, cx| {
            if let Some(edit) = shell.rename.edit.as_mut() {
                let away = edit.line.pressed_away.take();
                if std::mem::take(&mut edit.line.selecting) {
                    cx.stop_propagation();
                } else if let Some(at) = away {
                    shell.rename_press_away(at, window, cx);
                }
            }
        });
    });
}
