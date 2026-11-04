//! Backslash escapes and character references in destinations, info strings and running text, decoded here.

use super::markdown_entity_table::PACKED;
use super::markdown_text::at;
use std::sync::OnceLock;

const REPLACEMENT: char = '\u{fffd}';
// The longest reference is "&CounterClockwiseContourIntegral;" (33 characters), so a window this long holds any one.
const REFERENCE_WINDOW: usize = 34;
const MAX_NAME: usize = 32;
const MAX_DECIMAL: usize = 7;
const MAX_HEX: usize = 6;

/// The named references in name order, unpacked on first use.
fn names() -> &'static [(&'static str, String)] {
    static NAMES: OnceLock<Vec<(&'static str, String)>> = OnceLock::new();
    NAMES.get_or_init(|| {
        PACKED
            .split(' ')
            .filter_map(|entry| {
                let (name, points) = entry.split_once('=')?;
                let value = points
                    .split('.')
                    .filter_map(|hex| u32::from_str_radix(hex, 16).ok())
                    .filter_map(char::from_u32)
                    .collect();
                Some((name, value))
            })
            .collect()
    })
}

/// A named reference's text, as "copy" answers "\u{a9}"; an unknown name answers None.
pub fn named_value(name: &str) -> Option<&'static str> {
    let table = names();
    table
        .binary_search_by(|(key, _)| (*key).cmp(name))
        .ok()
        .map(|at| table[at].1.as_str())
}

/// A numeric reference's character: 0, a surrogate and a value past U+10FFFF answer U+FFFD.
pub fn code_point_text(code: u32) -> char {
    char::from_u32(code)
        .filter(|_| code > 0)
        .unwrap_or(REPLACEMENT)
}

/// One reference read at a "&": its decoded text and the index after its ";".
enum Reference {
    Numeric(char, usize),
    Named(&'static str, usize),
    // A well-formed name the table does not hold, which stays as written.
    Unknown(usize),
}

// Reads "&#digits;", "&#xhex;" or "&name;" at i, as the reference grammar bounds each.
fn read_reference(text: &str, i: usize) -> Option<Reference> {
    let bytes = text.as_bytes();
    if at(text, i) != b'&' {
        return None;
    }
    if at(text, i + 1) == b'#' {
        let hex = matches!(at(text, i + 2), b'x' | b'X');
        let start = if hex { i + 3 } else { i + 2 };
        let limit = if hex { MAX_HEX } else { MAX_DECIMAL };
        let mut j = start;
        while j < bytes.len()
            && j - start < limit
            && (if hex {
                bytes[j].is_ascii_hexdigit()
            } else {
                bytes[j].is_ascii_digit()
            })
        {
            j += 1;
        }
        if j == start || at(text, j) != b';' {
            return None;
        }
        let code = u32::from_str_radix(&text[start..j], if hex { 16 } else { 10 }).ok()?;
        return Some(Reference::Numeric(code_point_text(code), j + 1));
    }
    if !at(text, i + 1).is_ascii_alphabetic() {
        return None;
    }
    let start = i + 1;
    let mut j = start;
    while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
        j += 1;
    }
    if j - start < 2 || j - start > MAX_NAME || at(text, j) != b';' {
        return None;
    }
    Some(match named_value(&text[start..j]) {
        Some(value) => Reference::Named(value, j + 1),
        None => Reference::Unknown(j + 1),
    })
}

/// The text with escapes and references decoded: "f&ouml;o\\*" answers "f\u{f6}o*"; "&nosuch;" stays as written.
pub fn decode_references(text: &str) -> String {
    if !text.contains('\\') && !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut plain = 0;
    while i < text.len() {
        let b = text.as_bytes()[i];
        if b == b'\\' && super::markdown_escape::is_ascii_punct(at(text, i + 1) as u32) {
            out.push_str(&text[plain..i]);
            out.push(at(text, i + 1) as char);
            i += 2;
            plain = i;
            continue;
        }
        if b == b'&' {
            match read_reference(text, i) {
                Some(Reference::Numeric(c, end)) => {
                    out.push_str(&text[plain..i]);
                    out.push(c);
                    i = end;
                    plain = i;
                    continue;
                }
                Some(Reference::Named(value, end)) => {
                    out.push_str(&text[plain..i]);
                    out.push_str(value);
                    i = end;
                    plain = i;
                    continue;
                }
                Some(Reference::Unknown(end)) => {
                    i = end;
                    continue;
                }
                None => {}
            }
        }
        i += 1;
    }
    out.push_str(&text[plain..]);
    out
}

/// The reference at i decoded, and the index after it: "a &copy; b" at 2 answers ("\u{a9}", 8); unknown names answer None.
pub fn reference_at(text: &str, i: usize) -> Option<(String, usize)> {
    let window = super::markdown_text::slice(text, i, i + REFERENCE_WINDOW);
    match read_reference(window, 0)? {
        Reference::Numeric(c, end) => Some((c.to_string(), i + end)),
        Reference::Named(value, end) => Some((value.to_string(), i + end)),
        Reference::Unknown(_) => None,
    }
}

/// A tab, newline, carriage return or form feed a reference decoded to is drawn as one space.
pub fn drawn_text(text: &str) -> String {
    text.chars()
        .map(|c| {
            if matches!(c, '\t' | '\n' | '\r' | '\u{c}') {
                ' '
            } else {
                c
            }
        })
        .collect()
}
