//! A focused text or folder preview reads the listing's motion, Quick Look and quit keys; Tab stays as parity.tsv binds it.
use super::{input::Key, keymap::Map, model::Model, wire::Wire};
use std::io;

// The listing's action for a key the preview context leaves unbound, with the listing's prefixes and its bare q.
pub(super) fn listing_action(m: &Model, key: &Key, map: &Map, armed: &str, here: &str) -> String {
    if let Some(prefix) = armed.strip_prefix("prefix:") {
        return map.sequence(prefix, key, &m.preset, "listing");
    }
    if !here.is_empty() {
        return here.into();
    }
    let action = map.action(key, &m.preset);
    if action.is_empty() && key.mods.is_empty() && key.text == "q" {
        return "quit".into();
    }
    action
}

// Sample input: q with no binding in the pdf context answers "quit"; any other key answers None.
pub fn quit(m: &Model, key: &Key, map: &Map, here: &str) -> Option<String> {
    let action = listing_action(m, key, map, "", here);
    matches!(action.as_str(), "quit" | "quitNoCwd").then_some(action)
}

pub fn text(
    m: &mut Model,
    key: &Key,
    map: &Map,
    armed: &str,
    here: &str,
    w: &mut Wire,
) -> io::Result<()> {
    let action = listing_action(m, key, map, armed, here);
    // Only the jump prefixes arm here, so a listing prefix such as c or d never waits on a preview key.
    if action == "prefix:g" || action == "cursorFirstArm" && armed != action {
        m.key_arm = action;
        return Ok(());
    }
    let limit = super::layout::preview_scroll_limit(m);
    let page = super::pane::rows(m).max(1);
    let to = |m: &mut Model, by: isize| {
        m.preview_scroll = m.preview_scroll.saturating_add_signed(by).min(limit)
    };
    match action.as_str() {
        "cursorDown" => to(m, 1),
        "cursorUp" => to(m, -1),
        "pageDown" => to(m, page as isize),
        "pageUp" => to(m, -(page as isize)),
        "halfPageDown" => to(m, (page / 2).max(1) as isize),
        "halfPageUp" => to(m, -((page / 2).max(1) as isize)),
        "first" | "cursorFirst" | "cursorFirstArm" => m.preview_scroll = 0,
        "last" | "cursorLast" => m.preview_scroll = limit,
        "preview" if m.quicklook => m.quicklook = false,
        "preview" | "quit" | "quitNoCwd" | "keymapSheet" => {
            return super::actions::act(m, &action, w)
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
#[path = "preview_keys_tests.rs"]
mod tests;
