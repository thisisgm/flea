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

#[derive(Clone, Debug, Default)]
pub struct Keyboard {
    pub context: Context,
    sequence: Sequence,
    composing: bool,
    owner: Option<crate::window::text_input::Owner>,
    sequence_path: String,
}

impl Keyboard {
    pub(crate) fn capture_sequence_path(&mut self, path: &str) {
        self.sequence_path.clear();
        self.sequence_path.push_str(path);
    }

    pub(crate) fn sequence_path(&self) -> &str {
        &self.sequence_path
    }

    /// A composing field holds every key the window would read; an xkb dead key arrives with `prefer_character_input` instead.
    pub fn set_composing(&mut self, composing: bool) {
        self.composing = composing;
        self.sequence.clear();
    }

    /// The field `Shell::sync_text_input` last found holding marked text.
    pub(crate) fn text_owner(&self) -> Option<crate::window::text_input::Owner> {
        self.owner
    }

    /// The root's only caller of `set_composing`: a field holding marked text composes, and no owner composes nothing.
    pub(crate) fn own_text(&mut self, owner: Option<crate::window::text_input::Owner>) {
        self.owner = owner;
        self.set_composing(owner.is_some());
    }

    pub(crate) fn composing(&self, event: &KeyDownEvent) -> bool {
        self.composing || event.prefer_character_input
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
        mirrored: bool,
        seat: Seat,
    ) -> Dispatch {
        if self.composing(event) {
            self.sequence.clear();
            return Dispatch::Composition;
        }
        match event_lookup(name, self.context, event, held, mirrored) {
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

    /// The event's binding in `context` without touching the armed pair; text input binds nothing, as in `resolve`.
    pub fn lookup(
        &self,
        name: &str,
        context: Context,
        event: &KeyDownEvent,
        held: Modifiers,
        mirrored: bool,
    ) -> Match {
        if self.composing(event) {
            return Match::Unbound;
        }
        event_lookup(name, context, event, held, mirrored)
    }
}

pub fn modifiers(modifiers: Modifiers) -> u8 {
    (u8::from(modifiers.control) * keys::CONTROL)
        | (u8::from(modifiers.shift) * keys::SHIFT)
        | (u8::from(modifiers.alt) * keys::ALT)
        | (u8::from(modifiers.platform) * keys::SUPER)
}

/// The event's binding in i18n/keyboard-policy.json order: the US position first for digits and non-Latin groups, else the printed key, read as `reading_input` reads it.
fn event_lookup(
    name: &str,
    context: Context,
    event: &KeyDownEvent,
    held: Modifiers,
    mirrored: bool,
) -> Match {
    let mirrored = mirrored && reads_mirrored(context);
    let [first, second] =
        event_inputs(event, held).map(|input| input.map(|input| reading_input(input, mirrored)));
    match first {
        Some(first) => keys::lookup_either(name, context, Frontend::Gui, first, second),
        None => Match::Unbound,
    }
}

/// The event's two inputs in the order `event_lookup` tries them, so a user binding matches on every layout the table does.
pub fn event_inputs(event: &KeyDownEvent, held: Modifiers) -> [Option<Input<'_>>; 2] {
    let printed = event_input(event, held);
    match position_input(event, held) {
        Some((position, true)) => [Some(position), Some(printed)],
        position => [Some(printed), position.map(|(input, _)| input)],
    }
}

/// Spatial contexts follow a mirrored window; previews, PDFs, media and editors keep their keys, as file content does.
fn reads_mirrored(context: Context) -> bool {
    matches!(
        context,
        Context::Listing | Context::Rail | Context::Menu | Context::Panel
    )
}

/// Omarchy's readingKey, so a mirrored window swaps Left and Right with any modifiers and a plain h and l.
fn reading_input(input: Input<'_>, mirrored: bool) -> Input<'_> {
    if !mirrored {
        return input;
    }
    let plain = input.modifiers & keys::MODIFIER_MASK == 0;
    let key = match input.key {
        "h" | "l" if !plain => input.key,
        key => reading(key, true),
    };
    let text = if plain {
        reading(input.text, true)
    } else {
        input.text
    };
    Input { key, text, ..input }
}

/// The key a navigation reader acts on, with Left and Right and h and l swapped when mirrored, as Omarchy's PanelKeyCatcher does.
pub fn reading(key: &str, mirrored: bool) -> &str {
    match (key, mirrored) {
        ("left", true) => "right",
        ("right", true) => "left",
        ("h", true) => "l",
        ("l", true) => "h",
        (key, _) => key,
    }
}

// The US main-block keys, unshifted and with Shift, at the same index.
const US_UNSHIFTED: &str = "1234567890-=[];'`\\,./abcdefghijklmnopqrstuvwxyz";
const US_SHIFTED: &str = "!@#$%^&*()_+{}:\"~|<>?ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The key at the event's US position and whether it is tried first; `None` off the main block, for an AltGr press, and on a Latin group at a US letter key or for a printed ASCII letter, which match only where printed.
fn position_input(event: &KeyDownEvent, held: Modifiers) -> Option<(Input<'static>, bool)> {
    // An AltGr press is text input, so its character never falls back to the key under it.
    if event.prefer_character_input {
        return None;
    }
    let stroke = &event.keystroke;
    if misjudged_letter(stroke).is_some() {
        return None;
    }
    let us = stroke.us_key.as_deref()?;
    let index = (0..US_UNSHIFTED.len())
        .find(|&i| us == &US_UNSHIFTED[i..=i] || us == &US_SHIFTED[i..=i])?;
    let (lower, upper) = (&US_UNSHIFTED[index..=index], &US_SHIFTED[index..=index]);
    let typed = stroke.key_char.as_deref().unwrap_or("");
    // The fork reports the US key itself on a non-Latin group, so its typed character differs from the key.
    let nonlatin = stroke.key == us
        && !typed.is_empty()
        && !typed
            .chars()
            .flat_map(char::to_lowercase)
            .eq(stroke.key.chars());
    let letter = lower.bytes().all(|byte| byte.is_ascii_lowercase());
    let digit = lower.bytes().all(|byte| byte.is_ascii_digit());
    // ASCII only: rule b sends Ctrl+= to Italian ì and Ctrl+- to Icelandic ö, and a dead key's name is no letter.
    let mut printed = (if typed.is_empty() { &stroke.key } else { typed }).chars();
    let ascii_letter = printed
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && printed.next().is_none();
    if !nonlatin && (letter || ascii_letter) {
        return None;
    }
    let shifted = if letter {
        held.shift || stroke.modifiers.shift
    } else {
        us == upper
    };
    let text = if shifted { upper } else { lower };
    let key = table_key(
        if digit || letter || !shifted {
            lower
        } else {
            upper
        },
        false,
    );
    let modifiers = event_input(event, held).modifiers;
    Some((
        Input {
            key,
            text,
            modifiers,
        },
        digit || nonlatin,
    ))
}

/// A partial Latin keymap can fail the fork's Latin test; its printed ASCII letter still wins.
fn misjudged_letter(stroke: &gpui::Keystroke) -> Option<&str> {
    let typed = stroke.key_char.as_deref()?;
    let mut chars = typed.chars();
    let letter = chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.next().is_none();
    (letter
        && stroke.us_key.as_deref() == Some(stroke.key.as_str())
        && !typed.eq_ignore_ascii_case(&stroke.key))
    .then_some(typed)
}

pub fn event_input(event: &KeyDownEvent, held: Modifiers) -> Input<'_> {
    let keypad = event.keystroke.is_keypad;
    Input {
        key: misjudged_letter(&event.keystroke)
            .unwrap_or_else(|| table_key(&event.keystroke.key, keypad)),
        text: event.keystroke.key_char.as_deref().unwrap_or(""),
        modifiers: event_modifiers(event, held),
    }
}

// The key table's name for a fork key name.
fn table_key(key: &str, keypad: bool) -> &str {
    match key {
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
    }
}

fn event_modifiers(event: &KeyDownEvent, held: Modifiers) -> u8 {
    let mut mask = modifiers(event.keystroke.modifiers);
    if held.shift && caseless(&event.keystroke.key) {
        mask |= keys::SHIFT;
    }
    if event.keystroke.is_keypad {
        mask |= keys::KEYPAD;
    }
    mask
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

#[cfg(test)]
#[path = "keyboard_mirror_tests.rs"]
mod mirror_tests;
#[cfg(test)]
#[path = "keyboard_us_position_tests.rs"]
mod us_position_tests;
