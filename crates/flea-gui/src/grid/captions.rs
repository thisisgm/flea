//! Grid-only captions, ui/js/GridNames.js: Flea's own two-line wrap and elide, and the match run per drawn line.
use crate::row::unit_cells;
use std::{borrow::Cow, ops::Range};
use unicode_segmentation::UnicodeSegmentation;

/// A caption break stays behind a separator, so a word is never split when a break fits.
const BREAKS: [char; 4] = [' ', '-', '_', '.'];
const ELLIPSIS: &str = "\u{2026}";

// One grapheme cluster: its bytes and its Format.js cells, so a break never cuts a tall script's mark from its letter.
#[derive(Clone, Copy)]
struct Unit {
    start: usize,
    end: usize,
    cells: usize,
}

fn units(text: &str) -> Vec<Unit> {
    text.grapheme_indices(true)
        .map(|(at, unit)| Unit {
            start: at,
            end: at + unit.len(),
            cells: unit_cells(unit),
        })
        .collect()
}

// A unit is one separator alone, never a separator carrying a combining mark.
fn single(text: &str, unit: Unit, set: &[char]) -> bool {
    let mut chars = text[unit.start..unit.end].chars();
    matches!((chars.next(), chars.next()), (Some(ch), None) if set.contains(&ch))
}

fn cells(units: &[Unit], range: Range<usize>) -> usize {
    units[range].iter().map(|unit| unit.cells).sum()
}

// Elements from pos fitting in this many cells.
fn span_from(units: &[Unit], pos: usize, budget: usize) -> usize {
    let mut used = 0;
    units[pos..]
        .iter()
        .take_while(|unit| {
            used += unit.cells;
            used <= budget
        })
        .count()
}

// Trailing elements fitting in this many cells.
fn tail_span(units: &[Unit], budget: usize) -> usize {
    let mut used = 0;
    units
        .iter()
        .rev()
        .take_while(|unit| {
            used += unit.cells;
            used <= budget
        })
        .count()
}

// The extension is the last dot's tail; a leading dot names a dotfile, and a trailing one names nothing.
fn extension_length(text: &str, units: &[Unit]) -> usize {
    match (1..units.len())
        .rev()
        .find(|&at| single(text, units[at], &['.']))
    {
        Some(dot) if dot != units.len() - 1 => units.len() - dot,
        _ => 0,
    }
}

// The head takes the odd cell; the tail keeps a one-line extension whole when the mark still fits beside it.
fn elide_split(
    capacity: usize,
    extension: usize,
    extension_cells: usize,
    per_line: usize,
) -> (usize, usize) {
    let (mut head, mut tail) = ((capacity - 1).div_ceil(2), (capacity - 1) / 2);
    if extension > 0 && extension_cells <= per_line && extension_cells < capacity {
        tail = tail.max(extension_cells);
        head = capacity - 1 - tail;
    }
    (head, tail)
}

fn elide(text: &str, units: &[Unit], capacity: usize, per_line: usize) -> String {
    if capacity <= 1 {
        return ELLIPSIS.into();
    }
    if cells(units, 0..units.len()) <= capacity {
        return text.into();
    }
    let extension = extension_length(text, units);
    let extension_cells = cells(units, units.len() - extension..units.len());
    let (head, tail) = elide_split(capacity, extension, extension_cells, per_line);
    let head = span_from(units, 0, head);
    let tail = tail_span(units, tail);
    let head_end = units.get(head).map_or(text.len(), |unit| unit.start);
    let tail_start = units
        .get(units.len() - tail)
        .map_or(text.len(), |unit| unit.start);
    format!("{}{ELLIPSIS}{}", &text[..head_end], &text[tail_start..])
}

// The break a line takes: the last separator whose rest still fits, the extension's own dot losing to any earlier one.
fn break_cut(
    text: &str,
    units: &[Unit],
    far: usize,
    pos: usize,
    left: usize,
    per_line: usize,
    dot: usize,
) -> usize {
    let mut kept_dot = None;
    for at in (pos + 1..far).rev() {
        if !single(text, units[at], &BREAKS)
            || cells(units, at + 1..units.len()) > (left - 1) * per_line
        {
            continue;
        }
        if at == dot {
            kept_dot = Some(at + 1);
            continue;
        }
        return at + 1;
    }
    kept_dot.unwrap_or(far)
}

// Up to count lines of per_line cells as unit ranges, every line breaking after the last separator that leaves the rest fitting.
fn wrap(text: &str, units: &[Unit], per_line: usize, count: usize) -> Vec<Range<usize>> {
    let (mut out, mut pos) = (Vec::with_capacity(count), 0);
    let dot = units.len() - extension_length(text, units);
    for line in 0..count {
        if pos >= units.len() {
            break;
        }
        if cells(units, pos..units.len()) <= per_line {
            out.push(pos..units.len());
            break;
        }
        let far = pos + span_from(units, pos, per_line);
        if line == count - 1 {
            out.push(pos..far);
            break;
        }
        let cut = break_cut(
            text,
            units,
            far.max(pos + 1),
            pos,
            count - line,
            per_line,
            dot,
        );
        out.push(pos..cut);
        pos = cut;
    }
    out
}

fn joined(text: &str, units: &[Unit], lines: &[Range<usize>], separator: &str) -> String {
    let mut out = String::with_capacity(text.len() + lines.len());
    for (at, line) in lines.iter().enumerate() {
        if at > 0 {
            out.push_str(separator);
        }
        let start = units.get(line.start).map_or(text.len(), |unit| unit.start);
        let end = units.get(line.end).map_or(text.len(), |unit| unit.start);
        out.push_str(&text[start..end]);
    }
    out
}

/// GridNames.gridCaption: the caption holds its own breaks; a dead or non-finite budget hands the whole name back.
pub fn grid_caption(name: &str, per_line: f64, lines: f64) -> Cow<'_, str> {
    let (per, count) = (per_line.floor(), lines.floor());
    if !per.is_finite() || !count.is_finite() || per < 1. || count < 1. {
        return name.into();
    }
    let (per, count) = (per as usize, count as usize);
    let mut capacity = per * count;
    let source = units(name);
    if source.is_empty() || (capacity > 1 && cells(&source, 0..source.len()) <= per) {
        return name.into();
    }
    // A wide glyph never straddles a line, so elide until the wrap holds every unit.
    let mut shown = elide(name, &source, capacity, per);
    loop {
        let laid = units(&shown);
        let lines = wrap(&shown, &laid, per, count);
        if capacity <= 1 || joined(&shown, &laid, &lines, "") == shown {
            return joined(&shown, &laid, &lines, "\n").into();
        }
        capacity -= 1;
        shown = elide(name, &source, capacity, per);
    }
}

/// GridNames.lastLineCells: the cells of the caption's last line, so a mark sits past the glyphs it follows.
pub fn last_line_cells(caption: &str) -> usize {
    caption
        .rsplit('\n')
        .next()
        .map_or(0, |line| units(line).iter().map(|unit| unit.cells).sum())
}

/// Match.run: the first case-insensitive occurrence of the query, as a byte range of the haystack.
pub fn run(hay: &str, query: &str) -> Option<Range<usize>> {
    if query.is_empty() {
        return None;
    }
    let same = |a: char, b: char| a == b || a.to_lowercase().eq(b.to_lowercase());
    hay.char_indices().find_map(|(at, _)| {
        let mut end = at;
        let mut rest = hay[at..].chars();
        for wanted in query.chars() {
            let got = rest.next().filter(|&got| same(got, wanted))?;
            end += got.len_utf8();
        }
        Some(at..end)
    })
}

/// One drawn line's share of the match run, as byte ranges of that line; trailing blanks take no mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineMark {
    pub line: usize,
    pub run: Range<usize>,
    pub centred: usize,
}

/// GridNames.captionMarks: the run picked in the caption without its breaks, split per drawn line.
pub fn caption_marks(caption: &str, query: &str) -> Vec<LineMark> {
    let whole: String = caption.split('\n').collect();
    let Some(found) = run(&whole, query) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut pos = 0;
    for (line, text) in caption.split('\n').enumerate() {
        let centred = text
            .trim_end_matches(|ch: char| ch.is_whitespace() || ch == '\u{feff}')
            .len();
        let from = found.start.max(pos) - pos;
        let to = found.end.min(pos + centred).saturating_sub(pos);
        if from < to {
            out.push(LineMark {
                line,
                run: from..to,
                centred,
            });
        }
        pos += text.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_sample_and_the_samples_beside_it_break_as_gridnames_does() {
        let full = "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png";
        assert_eq!(
            grid_caption(full, 16., 2.),
            "screenshot-2026-\n\u{2026}he-bench-v3.png"
        );
        assert_eq!(
            grid_caption("my-vacation-1.jpeg", 16., 2.),
            "my-vacation-\n1.jpeg"
        );
        assert_eq!(
            grid_caption("a verylongnamewithoutanyspacesatall.png", 16., 2.),
            "a verylongnamewi\n\u{2026}spacesatall.png"
        );
        assert_eq!(grid_caption("IMG_4121.jpg", 16., 2.), "IMG_4121.jpg");
        assert_eq!(grid_caption("abcdef", f64::NAN, 2.), "abcdef");
        assert_eq!(last_line_cells("ab\nc\u{1f389}"), 3);
    }
}
