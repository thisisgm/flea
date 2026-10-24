//! Secret edits wipe replaced buffers and keep only positions outside the secret.
use crate::g2::pl1::secret::Secret;
use std::ops::Range;

pub fn units(secret: &Secret) -> usize {
    std::str::from_utf8(secret.expose())
        .unwrap_or("")
        .encode_utf16()
        .count()
}

pub fn replace(secret: &mut Secret, range: Range<usize>, text: &str) -> Range<usize> {
    let current = std::str::from_utf8(secret.expose()).unwrap_or("");
    let start = bytes(current, range.start);
    let start_units = current[..start].encode_utf16().count();
    let end = bytes(current, range.end.max(range.start));
    let mut next = Secret::from_bytes(&secret.expose()[..start]);
    next.push_str(text);
    next.push_str(&current[end..]);
    *secret = next;
    let at = start_units + text.encode_utf16().count();
    at..at
}

pub fn previous(secret: &Secret, at: usize) -> usize {
    let text = std::str::from_utf8(secret.expose()).unwrap_or("");
    let prefix = &text[..bytes(text, at)];
    prefix
        .encode_utf16()
        .count()
        .saturating_sub(prefix.chars().next_back().map_or(0, char::len_utf16))
}
pub fn next(secret: &Secret, at: usize) -> usize {
    let text = std::str::from_utf8(secret.expose()).unwrap_or("");
    at.saturating_add(
        text[bytes(text, at)..]
            .chars()
            .next()
            .map_or(0, char::len_utf16),
    )
    .min(units(secret))
}
pub fn display_index(secret: &Secret, at: usize, reveal: bool) -> usize {
    let text = std::str::from_utf8(secret.expose()).unwrap_or("");
    let prefix = &text[..bytes(text, at)];
    if reveal {
        prefix.len()
    } else {
        prefix.chars().count() * "\u{2022}".len()
    }
}

fn bytes(text: &str, units: usize) -> usize {
    let mut count = 0;
    for (index, c) in text.char_indices() {
        if count >= units {
            return index;
        }
        count += c.len_utf16();
    }
    text.len()
}

pub fn units_at_display_index(secret: &Secret, index: usize, reveal: bool) -> usize {
    let text = std::str::from_utf8(secret.expose()).unwrap_or("");
    if reveal {
        let mut index = index.min(text.len());
        while !text.is_char_boundary(index) {
            index = index.saturating_sub(1);
        }
        text[..index].encode_utf16().count()
    } else {
        text.chars()
            .take(index / "\u{2022}".len())
            .map(char::len_utf16)
            .sum()
    }
}
