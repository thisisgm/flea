//! How the sheet's keyboard spells keys: the held modifiers' words, and the character a US key types with Shift.
use super::{Face, SheetKeyboard};
use crate::keys::{ALT, CONTROL, SHIFT, SUPER};

/// The modifiers in the order a chord spells them: their key-table bit, the caps' word and the rest list's prefix.
const MODIFIERS: [(u8, &str, &str); 4] = [
    (CONTROL, "CTRL", "ctrl-"),
    (SHIFT, "SHIFT", "shift-"),
    (ALT, "ALT", "alt-"),
    (SUPER, "SUPER", "super-"),
];

/// The held modifiers as the keyboard's caps name them, in the order a chord spells them.
pub fn held_words(mask: u8) -> Vec<&'static str> {
    MODIFIERS
        .iter()
        .filter(|(bit, ..)| mask & bit != 0)
        .map(|(_, word, _)| *word)
        .collect()
}

/// The character a US key types with Shift: `!` for 1, `>` for `.` and for the ISO key beside Shift.
pub(super) fn us_shifted(us: &str) -> Option<&'static str> {
    const PLAIN: [&str; 22] = [
        "1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "=", "[", "]", "\\", ";", "'", ",",
        ".", "/", "`", "<",
    ];
    const SHIFTED: [&str; 22] = [
        "!", "@", "#", "$", "%", "^", "&", "*", "(", ")", "_", "+", "{", "}", "|", ":", "\"", "<",
        ">", "?", "~", ">",
    ];
    if let Some(at) = PLAIN.iter().position(|plain| *plain == us) {
        return Some(SHIFTED[at]);
    }
    let mut letters = us.chars();
    match (letters.next(), letters.next()) {
        (Some(letter), None) if letter.is_ascii_lowercase() => {
            const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
            let at = (letter as u8 - b'a') as usize;
            Some(&UPPER[at..=at])
        }
        _ => None,
    }
}

/// A list row's keys as the keyboard spells them: `shift-Ö / ctrl-l` reads `SHIFT + Ö / CTRL + L`, as the held list
/// and the corner spell a chord. A capital stands for its Shift chord and a two-key sequence names each key: `G`
/// reads `SHIFT + G`, `dd` reads `D D`.
pub fn spell(keys: &str) -> String {
    keys.split(" / ")
        .map(|alternative| {
            let parts: Vec<String> = alternative.split(' ').map(chord).collect();
            parts.join(" ")
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// One chord of a list row's keys, `ctrl-shift-k` or `G`, as the caps spell it.
fn chord(text: &str) -> String {
    let mut mask = 0;
    let mut rest = text;
    while let Some((bit, tail)) = MODIFIERS.iter().find_map(|(bit, _, prefix)| {
        rest.strip_prefix(prefix)
            .filter(|tail| !tail.is_empty())
            .map(|tail| (*bit, tail))
    }) {
        mask |= bit;
        rest = tail;
    }
    if let Some(name) = named(rest) {
        return joined(mask, name);
    }
    let keys: Vec<char> = rest.chars().collect();
    match keys.as_slice() {
        [key] => letter(mask, *key),
        // A modifier on an unnamed word keeps the word whole; a bare run of keys is a sequence, each key in turn.
        _ if mask != 0 => joined(mask, rest.to_uppercase()),
        _ => keys
            .iter()
            .map(|key| letter(0, *key))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// A single key: a capital is its letter with Shift, any other letter its capital legend, a symbol itself.
fn letter(mut mask: u8, key: char) -> String {
    if key.is_uppercase() {
        mask |= SHIFT;
    }
    joined(mask, key.to_uppercase().collect())
}

/// The held modifiers' words and the key, joined as a chord reads: `CTRL + SHIFT + C`.
fn joined(mask: u8, key: String) -> String {
    let mut words: Vec<String> = held_words(mask).into_iter().map(String::from).collect();
    words.push(key);
    words.join(" + ")
}

/// A named key as its cap's legend reads it.
fn named(name: &str) -> Option<String> {
    const NAMES: [(&str, &str); 29] = [
        ("escape", "ESC"),
        ("enter", "RETURN"),
        ("return", "RETURN"),
        ("backspace", "BKSP"),
        ("delete", "DEL"),
        ("insert", "INS"),
        ("pagedown", "PGDN"),
        ("pageup", "PGUP"),
        ("home", "HOME"),
        ("end", "END"),
        ("tab", "TAB"),
        ("backtab", "TAB"),
        ("space", "SPACE"),
        ("menu", "MENU"),
        ("up", "↑"),
        ("down", "↓"),
        ("left", "←"),
        ("right", "→"),
        ("comma", ","),
        ("period", "."),
        ("plus", "+"),
        ("minus", "-"),
        ("equal", "="),
        ("underscore", "_"),
        ("greater", ">"),
        ("less", "<"),
        ("slash", "/"),
        ("bracketleft", "["),
        ("bracketright", "]"),
    ];
    if let Some((_, legend)) = NAMES.iter().find(|(word, _)| *word == name) {
        return Some((*legend).to_owned());
    }
    // F-keys read as their caps do: f5 is F5.
    let number = name.strip_prefix('f')?;
    let digits =
        !number.is_empty() && number.len() <= 2 && number.bytes().all(|b| b.is_ascii_digit());
    digits.then(|| name.to_uppercase())
}

/// What a cap reads in a chord; a blank cap, such as a dead key, by the US position the table matched it on.
fn legend(face: &Face) -> &str {
    match face.cap.id {
        "SPCE" => "SPACE",
        _ if face.legend.is_empty() => face.cap.us,
        _ => &face.legend,
    }
}

/// A binding in the rest list's own spelling, `ctrl-shift-k`, for a search row the rest list has no row for. A named
/// cap keeps its key name, `insert`, so the list reads INS rather than spelling the legend out as a sequence.
pub fn canonical(face: &Face, mask: u8) -> String {
    let mut words: Vec<String> = held_words(mask).iter().map(|w| w.to_lowercase()).collect();
    words.push(match face.cap.key {
        "" => legend(face).to_lowercase(),
        key => key.to_owned(),
    });
    words.join("-")
}

impl SheetKeyboard {
    /// A chord as the keyboard's legends spell it, modifiers first: `CTRL + SHIFT + C`.
    pub fn chord(&self, mask: u8, cap: usize) -> String {
        joined(mask, legend(&self.faces[cap]).to_owned())
    }
}
