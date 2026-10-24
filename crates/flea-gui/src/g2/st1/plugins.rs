use super::view::SettingsView;
use crate::g2::st2::page::Key;
use gpui::{Context, KeyDownEvent};

pub(super) fn key(
    view: &mut SettingsView,
    event: &KeyDownEvent,
    cx: &mut Context<SettingsView>,
) -> bool {
    if view.panel.section != "plugins" || view.panel.side != super::Side::Pane {
        return false;
    }
    let Some(plugin) = &view.plugins else {
        return false;
    };
    let stroke = &event.keystroke;
    let key = match stroke.key.as_str() {
        "tab" => {
            return *plugin.read(cx).page().card() != crate::g2::st2::page::Card::None;
        }
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "enter" => Key::Enter,
        "escape" => Key::Escape,
        "space"
            if !matches!(
                plugin.read(cx).page().card(),
                crate::g2::st2::page::Card::Add(_)
            ) =>
        {
            Key::Space
        }
        "backspace" => Key::Backspace,
        _ => Key::Text(stroke.key_char.clone().unwrap_or_default()),
    };
    plugin.update(cx, |plugin, cx| plugin.key(key, cx));
    true
}
