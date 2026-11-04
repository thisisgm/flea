//! Right and left elision by display cells, as the oracle's Text elide modes cut a name and a location.
use crate::row::cells_of;
use std::borrow::Cow;

const ELLIPSIS: char = '\u{2026}';

fn width(text: &str) -> usize {
    text.chars().map(cells_of).sum()
}

/// Text.ElideRight: the head that fits, then an ellipsis.
pub fn right(text: &str, cells: usize) -> Cow<'_, str> {
    if width(text) <= cells {
        return Cow::Borrowed(text);
    }
    let mut used = 1;
    let head: String = text
        .chars()
        .take_while(|ch| {
            used += cells_of(*ch);
            used <= cells
        })
        .collect();
    Cow::Owned(if cells == 0 {
        String::new()
    } else {
        format!("{head}{ELLIPSIS}")
    })
}

/// Text.ElideLeft: an ellipsis, then the tail that fits, so a location keeps its last folders.
pub fn left(text: &str, cells: usize) -> Cow<'_, str> {
    if width(text) <= cells {
        return Cow::Borrowed(text);
    }
    let mut used = 1;
    let mut tail: Vec<char> = text
        .chars()
        .rev()
        .take_while(|ch| {
            used += cells_of(*ch);
            used <= cells
        })
        .collect();
    tail.reverse();
    Cow::Owned(if cells == 0 {
        String::new()
    } else {
        std::iter::once(ELLIPSIS).chain(tail).collect()
    })
}
