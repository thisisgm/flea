//! The query line's keys: a printable character types before the key table is read, so j and k never leave the line.
use super::filter::LineKey;
use crate::window::input::{event_input, paste_input, printable};
use flea::keys::{self, Context, Frontend, Input, Match};
use gpui::{KeyDownEvent, Modifiers};

// ui/js/Focus.js LEAVES_LINE: the cursor keys commit the line and then move the list.
const LEAVES_LINE: [&str; 6] = [
    "cursorDown",
    "cursorUp",
    "cursorFirst",
    "cursorLast",
    "pageDown",
    "pageUp",
];

/// A key the line owns, read through the generated key table for the listing; Paste asks the caller for the clipboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Classified<'a> {
    Paste,
    Key(LineKey<'a>),
}

pub fn classify<'a>(preset: &str, event: &'a KeyDownEvent, held: Modifiers) -> Classified<'a> {
    classify_input(preset, event_input(event, held))
}

pub fn classify_input<'a>(preset: &str, input: Input<'a>) -> Classified<'a> {
    if paste_input(input) {
        return Classified::Paste;
    }
    let chorded = input.modifiers & (keys::CONTROL | keys::ALT | keys::SUPER) != 0;
    let plain = input.modifiers & keys::MODIFIER_MASK == 0;
    let named = |name: &str| plain && input.key.eq_ignore_ascii_case(name);
    let key = match () {
        // The key table binds Tab only outside the listing; on the query line it is consumed, the scope's key once the recursive producer joins.
        _ if named("tab") => LineKey::Tab,
        _ if named("return") => LineKey::Enter,
        _ if named("escape") => LineKey::Escape,
        _ if named("backspace") => LineKey::Backspace,
        _ if !chorded && printable(input.text) => LineKey::Text(input.text),
        _ => match keys::lookup(preset, Context::Listing, Frontend::Gui, input) {
            Match::Action("selectAll") => LineKey::SelectAll,
            Match::Action("escape") => LineKey::Escape,
            Match::Action(action) if LEAVES_LINE.contains(&action) => LineKey::Leaves,
            _ => LineKey::Other,
        },
    };
    Classified::Key(key)
}
