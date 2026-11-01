use super::view::SettingsView;
use crate::g2::st2::page::Card;
use gpui::{Context, KeyDownEvent, Window};

pub(super) fn key(
    view: &mut SettingsView,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<SettingsView>,
) -> bool {
    if view.panel.query.is_some()
        || view.panel.section != "plugins"
        || view.panel.side != super::Side::Pane
    {
        return false;
    }
    let Some(plugin) = &view.plugins else {
        return false;
    };
    if event.keystroke.key == "tab" && *plugin.read(cx).page().card() == Card::None {
        return false;
    }
    plugin.update(cx, |plugin, cx| plugin.on_key(event, window, cx));
    true
}
