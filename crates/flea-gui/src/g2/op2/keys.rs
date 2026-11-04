//! The keys an archive or flash card answers, read from a native key event until the shared key table carries a dialog context.
use gpui::KeyDownEvent;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Tab,
    Backtab,
    Enter,
    Escape,
    Space,
    Backspace,
    Text(String),
}

impl Key {
    /// A mirrored window's card reads physical Left as Right and Right as Left, as Omarchy's PanelKeyCatcher does.
    pub fn read(self, mirrored: bool) -> Self {
        match (self, mirrored) {
            (Self::Left, true) => Self::Right,
            (Self::Right, true) => Self::Left,
            (key, _) => key,
        }
    }
}

/// Sample input: Shift+Tab reads Backtab; a printable key reads its own character; a held Enter or Space reads nothing.
pub fn key_of(event: &KeyDownEvent) -> Option<Key> {
    let stroke = &event.keystroke;
    if event.is_held && matches!(stroke.key.as_str(), "enter" | "space") {
        return None;
    }
    let shift = stroke.modifiers.shift;
    let plain = !stroke.modifiers.control && !stroke.modifiers.alt && !stroke.modifiers.platform;
    Some(match stroke.key.as_str() {
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "tab" if shift => Key::Backtab,
        "tab" => Key::Tab,
        "enter" => Key::Enter,
        "escape" => Key::Escape,
        "space" if plain => Key::Space,
        "backspace" => Key::Backspace,
        _ if plain => Key::Text(
            stroke
                .key_char
                .clone()
                .filter(|c| !c.chars().any(char::is_control))?,
        ),
        _ => return None,
    })
}

/// A text field's edit: a character appends, Backspace removes the last one.
pub fn edit(text: &mut String, key: &Key) -> bool {
    match key {
        Key::Text(typed) => text.push_str(typed),
        Key::Space => text.push(' '),
        Key::Backspace => {
            text.pop();
        }
        _ => return false,
    }
    true
}
