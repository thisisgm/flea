//! Private entries carry width-equivalent metadata while secret buffers retain and wipe every real byte.
use super::secret::Secret;
use crate::path::entry::{Entry, Kind};
use std::ops::Range;

pub(crate) fn shadow(text: &str) -> String {
    text.chars()
        .map(|ch| match ch.len_utf8() {
            1 => 'x',
            2 => '\u{a2}',
            3 => '\u{2022}',
            _ => '\u{10000}',
        })
        .collect()
}

pub(crate) fn begin(entry: &mut Entry, secret: &Secret) {
    entry.kind = Kind::Private;
    entry.begin(&shadow(std::str::from_utf8(secret.expose()).unwrap_or("")));
}

pub(crate) fn replace(
    entry: &mut Entry,
    secret: &mut Secret,
    range: Option<Range<usize>>,
    text: &str,
    marked: bool,
) -> bool {
    // A password takes no composition: the input method would show and learn the secret.
    if marked || text.chars().any(char::is_control) {
        return false;
    }
    let target = range
        .as_ref()
        .map(|r| entry.ime_range(r))
        .or_else(|| entry.marked.clone())
        .unwrap_or_else(|| entry.selected.clone());
    let current = secret.expose();
    if target.start > target.end || target.end > current.len() {
        return false;
    }
    let mut next = Secret::from_bytes(&current[..target.start]);
    next.push_str(text);
    next.push_str(std::str::from_utf8(&current[target.end..]).unwrap_or(""));
    entry.ime_replace(range, &shadow(text));
    *secret = next;
    true
}

pub(crate) fn erase(entry: &mut Entry, secret: &mut Secret, forward: bool) {
    if entry.selected.is_empty() {
        let cursor = entry.cursor();
        let to = if forward {
            entry.next(cursor)
        } else {
            entry.text[..cursor]
                .char_indices()
                .next_back()
                .map_or(0, |(at, _)| at)
        };
        entry.select_to(to);
    }
    replace(entry, secret, None, "", false);
}
