//! Translate native events without allocating, parsing tables or reading files.
use flea::keys::{
    self,
    sequence::{Seat, Sequence},
    Context, Frontend, Input, Match,
};
use gpui::{KeyDownEvent, Modifiers, MouseButton, MouseDownEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dispatch {
    Unbound,
    Suppressed,
    Armed,
    Composition,
    Action(&'static str),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Keyboard {
    pub context: Context,
    sequence: Sequence,
    composing: bool,
}

impl Keyboard {
    /// No caller yet: the text-input owner sets this from marked text, and Linux key events never prefer character input.
    pub fn set_composing(&mut self, composing: bool) {
        self.composing = composing;
        self.sequence.clear();
    }

    pub fn disarm(&mut self) {
        self.sequence.clear();
    }

    /// Whether a pair arm is live for this seat; read it before `resolve`, which disarms on any other key.
    pub fn pair_armed(&self, name: &str, seat: Seat) -> bool {
        self.sequence.live(seat, self.context, keys::preset(name))
    }

    /// The armed pair and its seat, for keyDeliveryState.
    pub fn pending(&self) -> Option<(&'static str, Seat)> {
        self.sequence.pending()
    }

    /// `held` is the window's modifier state, which keeps the Shift the platform drops from a symbol key.
    pub fn resolve(
        &mut self,
        name: &str,
        event: &KeyDownEvent,
        held: Modifiers,
        seat: Seat,
    ) -> Dispatch {
        if self.composing || event.prefer_character_input {
            self.sequence.clear();
            return Dispatch::Composition;
        }
        match keys::lookup(name, self.context, Frontend::Gui, event_input(event, held)) {
            Match::Unbound => {
                self.sequence.clear();
                Dispatch::Unbound
            }
            Match::Suppressed => {
                self.sequence.clear();
                Dispatch::Suppressed
            }
            Match::Action(action) => self
                .sequence
                .advance(action, seat, self.context, keys::preset(name))
                .map_or(Dispatch::Armed, Dispatch::Action),
        }
    }
}

pub fn modifiers(modifiers: Modifiers) -> u8 {
    (u8::from(modifiers.control) * keys::CONTROL)
        | (u8::from(modifiers.shift) * keys::SHIFT)
        | (u8::from(modifiers.alt) * keys::ALT)
        | (u8::from(modifiers.platform) * keys::SUPER)
}

pub fn event_input(event: &KeyDownEvent, held: Modifiers) -> Input<'_> {
    let keypad = event.keystroke.is_keypad;
    let key = match event.keystroke.key.as_str() {
        "enter" => "Return",
        "[" => "BracketLeft",
        "]" => "BracketRight",
        "," => "Comma",
        "." => "Period",
        "=" => "Equal",
        "+" => "Plus",
        "-" => "Minus",
        "_" => "Underscore",
        ">" => "Greater",
        // The fork names a keypad operator by its keysym; the key table names it by the Qt key for that keysym.
        "add" if keypad => "Plus",
        "subtract" if keypad => "Minus",
        "multiply" if keypad => "Asterisk",
        "divide" if keypad => "Slash",
        "separator" if keypad => "Comma",
        "decimal" if keypad => "Period",
        "equal" if keypad => "Equal",
        key => key,
    };
    let mut mask = modifiers(event.keystroke.modifiers);
    if held.shift && caseless(&event.keystroke.key) {
        mask |= keys::SHIFT;
    }
    if keypad {
        mask |= keys::KEYPAD;
    }
    Input {
        key,
        text: event.keystroke.key_char.as_deref().unwrap_or(""),
        modifiers: mask,
    }
}

// The Linux keystroke builder clears Shift on exactly these keys: one character with no case.
fn caseless(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|only| only.to_lowercase().eq(only.to_uppercase()))
        && chars.next().is_none()
}

pub fn pointer(
    place: &str,
    row: &str,
    event: &MouseDownEvent,
) -> Option<&'static keys::PointerBinding> {
    let button = match event.button {
        MouseButton::Left => "left",
        MouseButton::Right => "right",
        MouseButton::Middle => "middle",
        MouseButton::Navigate(gpui::NavigationDirection::Back) => "back",
        MouseButton::Navigate(gpui::NavigationDirection::Forward) => "forward",
    };
    keys::pointer(
        place,
        row,
        button,
        modifiers(event.modifiers),
        u8::try_from(event.click_count).ok()?,
    )
}

pub fn printable(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first >= ' ' && first != '\u{7f}' && first.len_utf16() == 1)
        && chars.next().is_none()
}

pub fn paste_key(event: &KeyDownEvent) -> bool {
    paste_input(event_input(event, event.keystroke.modifiers))
}

pub fn paste_input(input: Input<'_>) -> bool {
    let mask = input.modifiers & keys::MODIFIER_MASK;
    (mask == keys::CONTROL && input.key.eq_ignore_ascii_case("v"))
        || (mask == keys::SHIFT && input.key.eq_ignore_ascii_case("insert"))
}

pub fn clean_paste(text: &str) -> String {
    text.split(['\r', '\n'])
        .next()
        .unwrap_or("")
        .chars()
        .filter(|value| *value >= ' ' && *value != '\u{7f}')
        .collect()
}

pub fn paste_into(text: &str, type_text: impl FnOnce(&str)) -> bool {
    let text = clean_paste(text);
    if !text.is_empty() {
        type_text(&text);
    }
    true
}
