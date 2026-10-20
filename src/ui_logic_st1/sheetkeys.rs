// Keyboard sheet key decisions: key names, printable input and cursor steps.
use super::Text;

#[cfg(test)]
#[path = "sheetkeys_tests.rs"]
mod tests;

// What one key press does to the keyboard sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetKey {
    Close,
    Up,
    Down,
    Activate,
    Backspace,
    Type,
    Ignore,
}

// Shift, Control, Alt, AltGr, Meta and CapsLock: keys that carry no text on their own.
pub fn is_bare_modifier(key: &str) -> bool {
    matches!(key, "Shift" | "Control" | "Alt" | "AltGr" | "Meta" | "CapsLock")
}

// One printable character that is not DEL; a single UTF-16 unit, as the oracle counts length.
pub fn is_printable(text: &str) -> bool {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(ch), None) => ch.len_utf16() == 1 && ch >= ' ' && ch != '\u{7f}',
        _ => false,
    }
}

// The one decision the sheet's key handler runs; Delete and bare modifiers are ignored, never closed on.
pub fn sheet_key(query: &str, key: &str, text: &str) -> SheetKey {
    if key == "Escape" {
        return SheetKey::Close;
    }
    if !query.is_empty() {
        match key {
            "Up" => return SheetKey::Up,
            "Down" => return SheetKey::Down,
            "Return" | "Enter" => return SheetKey::Activate,
            _ => {}
        }
    }
    if key == "Backspace" {
        return if query.is_empty() { SheetKey::Close } else { SheetKey::Backspace };
    }
    if is_printable(text) {
        return SheetKey::Type;
    }
    if key == "Delete" || is_bare_modifier(key) {
        return SheetKey::Ignore;
    }
    SheetKey::Close
}

// Moves the cursor by delta, wrapping at both ends; with no rows it parks at 0.
pub fn step_cursor(cursor: i64, delta: i64, count: i64) -> i64 {
    if count <= 0 {
        return 0;
    }
    ((cursor + delta) % count + count) % count
}

// Actions that still run while a listing is out; every other action is swallowed.
const ANSWERED_WHILE_LISTING: [&str; 13] = [
    "open", "parent", "historyBack", "historyForward", "escape", "viewList", "viewColumns", "viewGrid",
    "viewDual", "sidebar", "windowNew", "togglePreview", "reload",
];

// What a sheet row run does: refuse with a message and keep the sheet open, or close and run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Run {
    Refuse(Text),
    Act,
}

// Whether the listing swap swallows an action while a listing is out.
fn swallows(list_in_flight: bool, action: &str) -> bool {
    list_in_flight && !action.is_empty() && !ANSWERED_WHILE_LISTING.contains(&action)
}

// Enter's action path: a swallowed action refuses with the loading message, any other closes then runs.
pub fn run_decision(list_in_flight: bool, action: &str) -> Run {
    if swallows(list_in_flight, action) {
        Run::Refuse(Text::Key("settings.sheet.loading"))
    } else {
        Run::Act
    }
}

// Enter's menu path: a menu row refuses for as long as any listing is out.
pub fn menu_run_decision(list_in_flight: bool) -> Run {
    if list_in_flight {
        Run::Refuse(Text::Key("settings.sheet.loading"))
    } else {
        Run::Act
    }
}
