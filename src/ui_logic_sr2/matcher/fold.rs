//! The name lowered whole, as the backend and JS toLowerCase lower it, with each lowered char mapped back to the name.
use super::{Matcher, Name, Run};

impl Matcher {
    // The name lowered as one string, as the backend lowers it, each lowered char keeping the name char it came from.
    pub(super) fn lower(&mut self, name: Name) {
        self.lowered.clear();
        self.origin.clear();
        if name.is_ascii() {
            self.lowered.extend(name.bytes().map(|b| char::from(b.to_ascii_lowercase())));
            self.origin.extend(0..name.len());
            return;
        }
        // Only a capital sigma lowers by its context, so only a name holding one is lowered whole.
        let whole = name.text().contains('\u{3a3}').then(|| name.text().to_lowercase());
        let mut context = whole.as_deref().map(str::chars);
        for (at, c) in name.chars().enumerate() {
            match context.as_mut() {
                Some(chars) => {
                    let width = if c == '\u{3a3}' { 1 } else { c.to_lowercase().count() };
                    self.lowered.extend(chars.by_ref().take(width));
                    self.origin.resize(self.lowered.len(), at);
                }
                None => {
                    self.lowered.extend(c.to_lowercase());
                    self.origin.resize(self.lowered.len(), at);
                }
            }
        }
    }

    // A stretch of the lowered name as the name's own chars, so a run never lands inside an expanded capital.
    pub(super) fn span(&self, start: usize, len: usize) -> Run {
        let chars = self.origin.last().map_or(0, |&at| at + 1);
        let first = self.origin.get(start).copied().unwrap_or(chars);
        let end = if len == 0 { first } else { self.origin[start + len - 1] + 1 };
        Run { start: first, len: end - first }
    }
}
