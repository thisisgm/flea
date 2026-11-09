//! Right-to-left hint units: a left-to-right run stays one unit and no unit keeps the paragraph mark, which shapes "++Ctrl+Shift".
use super::geometry::hint_units;
use unicode_bidi::{BidiInfo, Level};

/// The hint's units as the pane places them in `width`; right to left, a run longer than a line breaks at its own spaces.
pub fn hint_units_in(
    text: &str,
    rtl: bool,
    width: f32,
    measure: impl Fn(&str) -> f32,
) -> Vec<String> {
    if !rtl {
        return hint_units(text);
    }
    let mut out = Vec::new();
    // The width already on the line the current chunk starts on, packed as hint_line_count packs units.
    let mut used = 0.;
    for run in runs(text) {
        let mut chunk = String::new();
        for unit in run {
            if chunk.is_empty() {
                if used > 0. && used + measure(&unit) > width {
                    used = 0.;
                }
                chunk = unit;
                continue;
            }
            let longer = format!("{chunk}{unit}");
            if used + measure(&longer) <= width {
                chunk = longer;
                continue;
            }
            out.push(std::mem::replace(&mut chunk, unit));
            used = 0.;
        }
        used += measure(&chunk);
        out.push(chunk);
    }
    out
}

/// The hint's units grouped into runs: a left-to-right phrase is one run of its words, every other unit a run of its own.
fn runs(text: &str) -> Vec<Vec<String>> {
    let levels = BidiInfo::new(text, Some(Level::rtl())).levels;
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut at = 0;
    // The level of the space after the previous unit's word: an even one sits inside a left-to-right run.
    let mut joined = false;
    for unit in hint_units(text) {
        let word = unit.trim_end_matches(' ');
        let start = text[at..].find(word).map_or(at, |found| at + found);
        at = start + word.len();
        let unit = unit.trim_start_matches('\u{200f}');
        if unit.is_empty() {
            continue;
        }
        match out.last_mut() {
            Some(last) if joined => last.push(unit.to_owned()),
            _ => out.push(vec![unit.to_owned()]),
        }
        joined = unit.ends_with(' ') && levels.get(at).is_some_and(|level| level.is_ltr());
    }
    out
}

/// A unit's word and whether a space follows it; the space is placed beside the word, never shaped with it.
pub fn spaced(unit: &str) -> (&str, bool) {
    match unit.strip_suffix(' ') {
        Some(word) => (word, true),
        None => (unit, false),
    }
}

#[cfg(test)]
#[path = "hint_bidi_tests.rs"]
mod tests;
