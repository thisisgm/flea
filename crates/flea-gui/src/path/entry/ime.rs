//! The platform text contract every mounted field shares: UTF-16 ranges, composition and the keys it owns.
use super::{Entry, Kind};
use gpui::UTF16Selection;
use std::borrow::Cow;
use std::ops::Range;

const BULLET: &str = "\u{2022}";

impl Entry {
    /// The platform's UTF-16 range in bytes, widened outward so it never splits a character.
    pub fn ime_range(&self, range: &Range<usize>) -> Range<usize> {
        let mut units = 0;
        let start = self
            .text
            .char_indices()
            .find(|(_, ch)| {
                units += ch.len_utf16();
                units > range.start
            })
            .map_or(self.text.len(), |(at, _)| at);
        start..self.offset_from_utf16(range.end).max(start)
    }

    pub fn ime_selection(&self) -> UTF16Selection {
        UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        }
    }

    pub fn ime_marked(&self) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    /// The selection a field washes; the input method's highlight inside its composition is none, so the caret stays at its end.
    pub fn washed(&self) -> Option<Range<usize>> {
        let converting = self.marked.as_ref().is_some_and(|marked| {
            marked.start <= self.selected.start && self.selected.end <= marked.end
        });
        (!self.selected.is_empty() && !converting).then(|| self.selected.clone())
    }

    /// Surrounding text for the input method; a private line answers nothing.
    pub fn ime_text(
        &self,
        range: &Range<usize>,
        adjusted: &mut Option<Range<usize>>,
    ) -> Option<String> {
        if self.kind == Kind::Private {
            return None;
        }
        let range = self.ime_range(range);
        adjusted.replace(self.range_to_utf16(&range));
        self.text.get(range).map(str::to_owned)
    }

    /// Whether the input method may serve this line: a password or a number keeps it off, as v0.3.12 does.
    pub fn composes(&self) -> bool {
        self.kind == Kind::Text
    }

    /// The line without its composition, the text a consumer acts on; the field still paints the preedit.
    pub fn committed(&self) -> Cow<'_, str> {
        match &self.marked {
            Some(marked) => {
                Cow::Owned([&self.text[..marked.start], &self.text[marked.end..]].concat())
            }
            None => Cow::Borrowed(&self.text),
        }
    }

    /// Commits platform text; control characters never enter the line, and a number takes only digits.
    pub fn ime_replace(&mut self, range: Option<Range<usize>>, text: &str) -> bool {
        if text.chars().any(char::is_control)
            || (self.kind == Kind::Digits && !text.chars().all(|ch| ch.is_ascii_digit()))
        {
            return false;
        }
        let range = range.map(|range| self.ime_range(&range));
        self.replace(range, text);
        true
    }

    /// Composing text, its selection counted in UTF-16 units of `text`.
    pub fn ime_mark(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
    ) -> bool {
        if !self.composes() || text.chars().any(char::is_control) {
            return false;
        }
        let range = range.map(|range| self.ime_range(&range));
        // A unit inside a surrogate pair rounds past its character, as the line's own conversion does.
        let bytes = |units: usize| {
            let mut count = 0;
            text.char_indices()
                .find(|(_, ch)| {
                    let reached = count >= units;
                    count += ch.len_utf16();
                    reached
                })
                .map_or(text.len(), |(at, _)| at)
        };
        let selected = selected.map(|inner| bytes(inner.start)..bytes(inner.end));
        self.replace_and_mark(range, text, selected);
        true
    }

    pub fn ime_unmark(&mut self) -> bool {
        self.marked.take().is_some()
    }

    /// While text is marked Enter belongs to the input method, Escape drops the composition, and Tab commits it and moves on.
    pub fn ime_key(&mut self, key: &str) -> bool {
        let Some(marked) = self.marked.clone() else {
            return false;
        };
        match key {
            "escape" => self.replace(Some(marked), ""),
            "enter" | "return" | "kp_enter" => {}
            "tab" | "backtab" => {
                self.marked = None;
                return false;
            }
            _ => return false,
        }
        true
    }

    /// What paint shapes: the line, or one bullet per character when private.
    pub fn display(&self) -> Cow<'_, str> {
        if self.kind == Kind::Private {
            BULLET.repeat(self.text.chars().count()).into()
        } else {
            Cow::Borrowed(&self.text)
        }
    }

    /// A byte offset of the line as an offset into display().
    pub fn display_offset(&self, offset: usize) -> usize {
        let offset = floor(&self.text, offset);
        if self.kind == Kind::Private {
            self.text[..offset].chars().count() * BULLET.len()
        } else {
            offset
        }
    }

    /// A byte offset into display() as an offset of the line.
    pub fn from_display(&self, index: usize) -> usize {
        if self.kind == Kind::Private {
            let count = index / BULLET.len();
            let text = &self.text;
            text.char_indices()
                .nth(count)
                .map_or(text.len(), |(at, _)| at)
        } else {
            floor(&self.text, index)
        }
    }
}

fn floor(text: &str, offset: usize) -> usize {
    let offset = offset.min(text.len());
    (0..=offset)
        .rev()
        .find(|at| text.is_char_boundary(*at))
        .unwrap_or(0)
}

// Inclusive pairs of non-strong bidi classes (AN, ET, CS, ES, ON, BN and marks) inside the right-to-left blocks.
const RTL_WEAK: [u32; 82] = [
    0x0600, 0x0607, 0x0609, 0x060A, 0x060C, 0x060C, 0x060E, 0x060F, 0x066A, 0x066C, 0x06DD, 0x06DE,
    0x06E9, 0x06E9, 0x0711, 0x0711, 0x0730, 0x074A, 0x07A6, 0x07B0, 0x07EB, 0x07F3, 0x07F6, 0x07F9,
    0x07FD, 0x07FD, 0x0816, 0x0819, 0x081B, 0x0823, 0x0825, 0x0827, 0x0829, 0x082D, 0x0859, 0x085B,
    0x0890, 0x0891, 0x0897, 0x089F, 0x08CA, 0x08FF, 0xFB1E, 0xFB1E, 0xFB29, 0xFB29, 0xFD3E, 0xFD4F,
    0xFDCF, 0xFDCF, 0xFDFD, 0xFDFF, 0xFEFF, 0xFEFF, 0x10A01, 0x10A0F, 0x10A38, 0x10A3F, 0x10AE5,
    0x10AE6, 0x10D24, 0x10D27, 0x10D30, 0x10D39, 0x10D69, 0x10D6D, 0x10E60, 0x10E7E, 0x10EAB,
    0x10EAC, 0x10EFC, 0x10EFF, 0x10F46, 0x10F50, 0x10F82, 0x10F85, 0x1E8D0, 0x1E8D6, 0x1E944,
    0x1E94A, 0x1EEF0, 0x1EEF1,
];

/// Whether text's first strongly directed character runs right to left, the paragraph direction the shaper lays out.
pub fn right_to_left(text: &str) -> bool {
    text.chars()
        .find_map(|ch| {
            let code = ch as u32;
            let rtl = matches!(code, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF);
            // Marks and digits are weak in every block, and the table covers the rest.
            let weak = extends(ch)
                || ch.is_numeric()
                || RTL_WEAK.chunks(2).any(|pair| (pair[0]..=pair[1]).contains(&code));
            if rtl {
                (!weak).then_some(true)
            } else {
                ch.is_alphabetic().then_some(false)
            }
        })
        .unwrap_or(false)
}

// Inclusive pairs of marks, selectors and modifiers that ride on the character before; Brahmic rows are exact Mn and Mc signs.
const EXTENDERS: [u32; 264] = [
    0x0300, 0x036F, 0x0483, 0x0489, 0x0591, 0x05BD, 0x05BF, 0x05BF, 0x05C1, 0x05C2, 0x05C4, 0x05C5,
    0x05C7, 0x05C7, 0x0610, 0x061A, 0x064B, 0x065F, 0x0670, 0x0670, 0x06D6, 0x06DC, 0x06DF, 0x06E4,
    0x06E7, 0x06E8, 0x06EA, 0x06ED, 0x0711, 0x0711, 0x0730, 0x074A, 0x07A6, 0x07B0, 0x07EB, 0x07F3,
    0x07FD, 0x07FD, 0x0816, 0x0819, 0x081B, 0x0823, 0x0825, 0x0827, 0x0829, 0x082D, 0x0859, 0x085B,
    0x0897, 0x089F, 0x08CA, 0x08E1, 0x08E3, 0x08FF, 0x0900, 0x0903, 0x093A, 0x093C, 0x093E, 0x094F,
    0x0951, 0x0957, 0x0962, 0x0963, 0x0981, 0x0983, 0x09BC, 0x09BC, 0x09BE, 0x09C4, 0x09C7, 0x09C8,
    0x09CB, 0x09CD, 0x09D7, 0x09D7, 0x09E2, 0x09E3, 0x09FE, 0x09FE, 0x0A01, 0x0A03, 0x0A3C, 0x0A3C,
    0x0A3E, 0x0A42, 0x0A47, 0x0A48, 0x0A4B, 0x0A4D, 0x0A51, 0x0A51, 0x0A70, 0x0A71, 0x0A75, 0x0A75,
    0x0A81, 0x0A83, 0x0ABC, 0x0ABC, 0x0ABE, 0x0AC5, 0x0AC7, 0x0AC9, 0x0ACB, 0x0ACD, 0x0AE2, 0x0AE3,
    0x0AFA, 0x0AFF, 0x0B01, 0x0B03, 0x0B3C, 0x0B3C, 0x0B3E, 0x0B44, 0x0B47, 0x0B48, 0x0B4B, 0x0B4D,
    0x0B55, 0x0B57, 0x0B62, 0x0B63, 0x0B82, 0x0B82, 0x0BBE, 0x0BC2, 0x0BC6, 0x0BC8, 0x0BCA, 0x0BCD,
    0x0BD7, 0x0BD7, 0x0C00, 0x0C04, 0x0C3C, 0x0C3C, 0x0C3E, 0x0C44, 0x0C46, 0x0C48, 0x0C4A, 0x0C4D,
    0x0C55, 0x0C56, 0x0C62, 0x0C63, 0x0C81, 0x0C83, 0x0CBC, 0x0CBC, 0x0CBE, 0x0CC4, 0x0CC6, 0x0CC8,
    0x0CCA, 0x0CCD, 0x0CD5, 0x0CD6, 0x0CE2, 0x0CE3, 0x0CF3, 0x0CF3, 0x0D00, 0x0D03, 0x0D3B, 0x0D3C,
    0x0D3E, 0x0D44, 0x0D46, 0x0D48, 0x0D4A, 0x0D4D, 0x0D57, 0x0D57, 0x0D62, 0x0D63, 0x0D81, 0x0D83,
    0x0DCA, 0x0DCA, 0x0DCF, 0x0DD4, 0x0DD6, 0x0DD6, 0x0DD8, 0x0DDF, 0x0DF2, 0x0DF3, 0x0E31, 0x0E31,
    0x0E34, 0x0E3A, 0x0E47, 0x0E4E, 0x0EB1, 0x0EB1, 0x0EB4, 0x0EBC, 0x0EC8, 0x0ECD, 0x0F71, 0x0F84,
    0x0F86, 0x0F87, 0x0F8D, 0x0FBC, 0x102B, 0x103E, 0x1056, 0x1059, 0x105E, 0x1060, 0x1062, 0x1064,
    0x1067, 0x106D, 0x1071, 0x1074, 0x1082, 0x108D, 0x108F, 0x108F, 0x109A, 0x109D, 0x1160, 0x11FF,
    0x17B4, 0x17D3, 0x17DD, 0x17DD, 0x1AB0, 0x1AFF, 0x1CD0, 0x1CD2, 0x1CD4, 0x1CE8, 0x1CED, 0x1CED,
    0x1CF4, 0x1CF4, 0x1CF7, 0x1CF9, 0x1DC0, 0x1DFF, 0x200C, 0x200D, 0x20D0, 0x20FF, 0x302A, 0x302F,
    0x3099, 0x309A, 0xD7B0, 0xD7FF, 0xFE00, 0xFE0F, 0xFE20, 0xFE2F, 0x1F3FB, 0x1F3FF, 0xE0020,
    0xE01EF,
];
// Each linking virama Unicode segmentation names, with the consonant ranges (InCB=Consonant) of its script it joins.
const CONSONANTS: [(char, u32, u32); 26] = [
    ('\u{94d}', 0x0915, 0x0939),
    ('\u{94d}', 0x0958, 0x095F),
    ('\u{94d}', 0x0978, 0x097F),
    ('\u{9cd}', 0x0995, 0x09A8),
    ('\u{9cd}', 0x09AA, 0x09B0),
    ('\u{9cd}', 0x09B2, 0x09B2),
    ('\u{9cd}', 0x09B6, 0x09B9),
    ('\u{9cd}', 0x09DC, 0x09DD),
    ('\u{9cd}', 0x09DF, 0x09DF),
    ('\u{9cd}', 0x09F0, 0x09F1),
    ('\u{acd}', 0x0A95, 0x0AA8),
    ('\u{acd}', 0x0AAA, 0x0AB0),
    ('\u{acd}', 0x0AB2, 0x0AB3),
    ('\u{acd}', 0x0AB5, 0x0AB9),
    ('\u{acd}', 0x0AF9, 0x0AF9),
    ('\u{b4d}', 0x0B15, 0x0B28),
    ('\u{b4d}', 0x0B2A, 0x0B30),
    ('\u{b4d}', 0x0B32, 0x0B33),
    ('\u{b4d}', 0x0B35, 0x0B39),
    ('\u{b4d}', 0x0B5C, 0x0B5D),
    ('\u{b4d}', 0x0B5F, 0x0B5F),
    ('\u{b4d}', 0x0B71, 0x0B71),
    ('\u{c4d}', 0x0C15, 0x0C28),
    ('\u{c4d}', 0x0C2A, 0x0C39),
    ('\u{c4d}', 0x0C58, 0x0C5A),
    ('\u{d4d}', 0x0D15, 0x0D3A),
];

pub(super) fn extends(ch: char) -> bool {
    let code = ch as u32;
    EXTENDERS
        .chunks(2)
        .any(|pair| (pair[0]..=pair[1]).contains(&code))
}

// A linking virama binds a following consonant of its script, and a zero width joiner binds a following emoji or letter.
pub(super) fn binds(joiner: char, next: char) -> bool {
    let code = next as u32;
    if joiner == '\u{200d}' {
        !next.is_ascii() && !next.is_whitespace()
    } else {
        CONSONANTS
            .iter()
            .any(|&(linker, low, high)| linker == joiner && (low..=high).contains(&code))
    }
}
