//! What a key does on the unlock card: Return, Enter and Space press the focused button once, never on a held repeat.
use super::unlock::{Card, Part};

/// One key as the card reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key<'a> {
    pub name: &'a str,
    pub text: Option<&'a str>,
    pub shift: bool,
    pub plain: bool,
    pub held: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    Close,
    Submit,
    Tab { back: bool },
    Backspace,
    Type(String),
    Ignore,
}

/// The card's answer to a key; a key it does not take goes on to the window.
pub fn act(card: &Card, key: Key) -> Act {
    let field = card.focus == Some(Part::Field);
    let press = |part: Option<Part>| match part {
        Some(Part::Cancel) => Act::Close,
        Some(Part::Unlock) | Some(Part::Field) => Act::Submit,
        None => Act::Ignore,
    };
    match key.name {
        "escape" => Act::Close,
        "enter" | "return" if key.held => Act::Ignore,
        "enter" | "return" => press(card.focus),
        "space" if !field && key.held => Act::Ignore,
        "space" if !field => press(card.focus),
        "tab" => Act::Tab { back: key.shift },
        "backspace" if field => Act::Backspace,
        _ => match key.text {
            Some(text) if field && key.plain && !text.is_empty() => Act::Type(text.to_string()),
            _ => Act::Ignore,
        },
    }
}
