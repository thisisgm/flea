//! Test input delivery: text, keys, pastes, pointer and wheel controls into one mounted window.
use super::wire::{coordinate, word};
use flea::jsondoc::Json;
use flea_gui::window::root::Root;
use gpui::{
    point, px, ClipboardItem, Keystroke, Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent,
    TestAppContext, TouchPhase, VisualTestContext, WindowHandle,
};

// Delivers one input control; Ok(false) means the op is not an input control.
pub(super) fn deliver(
    cx: &mut TestAppContext,
    window: WindowHandle<Root>,
    op: &str,
    command: &Json,
) -> Result<bool, String> {
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    match op {
        "text" => {
            for character in word(command, "text")?.chars() {
                cx.dispatch_keystroke(
                    window.into(),
                    Keystroke {
                        key: "text".into(),
                        key_char: Some(character.to_string()),
                        ..Default::default()
                    },
                );
            }
        }
        "key" | "paste" => {
            let mut key = match op {
                "paste" => {
                    cx.write_to_clipboard(ClipboardItem::new_string(word(command, "text")?.into()));
                    Keystroke::parse(
                        command
                            .get("key")
                            .and_then(Json::as_str)
                            .unwrap_or("ctrl-v"),
                    )
                    .map_err(|e| e.to_string())?
                }
                _ => Keystroke::parse(word(command, "key")?).map_err(|e| e.to_string())?,
            };
            if op == "key" && key.key.chars().count() == 1 && key.modifiers == Modifiers::default()
            {
                key.key_char = Some(key.key.clone());
            }
            cx.dispatch_keystroke(window.into(), key);
        }
        "mouse" => {
            let position = point(px(coordinate(command, "x")?), px(coordinate(command, "y")?));
            let modifiers = Keystroke::parse(
                command
                    .get("modifiers")
                    .and_then(Json::as_str)
                    .unwrap_or("a"),
            )
            .map_err(|e| e.to_string())?
            .modifiers;
            let button = match command
                .get("button")
                .and_then(Json::as_str)
                .unwrap_or("left")
            {
                "left" => MouseButton::Left,
                "right" => MouseButton::Right,
                "middle" => MouseButton::Middle,
                _ => return Err("invalid mouse button".into()),
            };
            match word(command, "phase")? {
                "move" => visual.simulate_mouse_move(position, None, modifiers),
                // A move with the button still held, so a band or a drag that cancels on a bare move can form.
                "drag" => visual.simulate_mouse_move(position, Some(button), modifiers),
                "down" => visual.simulate_mouse_down(position, button, modifiers),
                "up" => visual.simulate_mouse_up(position, button, modifiers),
                "click" => {
                    visual.simulate_mouse_down(position, button, modifiers);
                    visual.simulate_mouse_up(position, button, modifiers);
                }
                _ => return Err("invalid mouse phase".into()),
            }
        }
        "wheel" => visual.simulate_event(ScrollWheelEvent {
            position: point(px(coordinate(command, "x")?), px(coordinate(command, "y")?)),
            delta: ScrollDelta::Pixels(point(
                px(coordinate(command, "dx")?),
                px(coordinate(command, "dy")?),
            )),
            modifiers: Modifiers::default(),
            touch_phase: TouchPhase::Moved,
        }),
        "focus" => match command.get("active").and_then(Json::as_bool) {
            Some(true) => visual.update(|window, _| window.activate_window()),
            Some(false) => visual.deactivate_window(),
            None => return Err("missing boolean active".into()),
        },
        _ => return Ok(false),
    }
    Ok(true)
}
