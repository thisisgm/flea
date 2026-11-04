//! Action ids outside code may run: built-ins from the key table, and `plugin:action` for plugin menu actions.
use super::{Binding, Context, Frontend, DIGITS, PRESET_KEYS, SEQUENCES, SHARED_KEYS};

/// Outcome statuses PLUGINS.md documents for `flea --action`; the window answers RAN, UNKNOWN or UNAVAILABLE itself.
pub const RAN: i32 = 0;
pub const USAGE: i32 = 2;
pub const UNKNOWN: i32 = 3;
pub const UNAVAILABLE: i32 = 4;
pub const NO_WINDOW: i32 = 5;

/// What an action id names; a built-in never contains a colon, a plugin id always does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Id<'a> {
    Builtin(&'static str),
    Plugin { plugin: &'a str, action: &'a str },
}

fn gui(row: &Binding) -> bool {
    row.frontend == "all" || row.frontend == Frontend::Gui.name()
}

// A pair arm only starts a two-key sequence; the action it completes is the id, never the arm.
fn arm(action: &str) -> bool {
    SEQUENCES.iter().any(|row| row.arm == action)
}

/// Every built-in id a key can run in the window, sorted and without repeats.
pub fn builtin_ids() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = PRESET_KEYS
        .iter()
        .chain(SHARED_KEYS)
        .filter(|row| gui(row))
        .map(|row| row.action)
        .chain(DIGITS.iter().map(|digit| digit.action))
        .chain(SEQUENCES.iter().map(|row| row.action))
        .filter(|action| !action.is_empty() && !arm(action))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The id's kind, or None for a word that names no built-in and no plugin action shape.
// Sample input: "viewGrid" or "example.checksums:sha256"
pub fn parse(id: &str) -> Option<Id<'_>> {
    if let Some((plugin, action)) = id.split_once(':') {
        let word = |s: &str| {
            !s.is_empty() && !s.contains([':', '/', '\0']) && !s.chars().any(char::is_whitespace)
        };
        return (word(plugin) && word(action)).then_some(Id::Plugin { plugin, action });
    }
    builtin_ids()
        .into_iter()
        .find(|known| *known == id)
        .map(Id::Builtin)
}

/// Whether a key in this context could reach the action, which is when the window would run it for a key press.
pub fn available_in(action: &str, context: Context) -> bool {
    let sequenced = |row: &Binding| {
        row.action == action
            || SEQUENCES
                .iter()
                .any(|s| s.arm == row.action && s.action == action)
    };
    let shared = matches!(context, Context::Listing | Context::Rail);
    PRESET_KEYS
        .iter()
        .any(|row| gui(row) && row.applies(context, Frontend::Gui) && sequenced(row))
        || (shared && SHARED_KEYS.iter().any(|row| gui(row) && sequenced(row)))
        || (shared && DIGITS.iter().any(|digit| digit.action == action))
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
