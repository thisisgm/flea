//! A code line laid out in cells of the monospace face: the gutter's width, the wrap at a word boundary or anywhere,
//! and the ellipsis at the view's edge when long lines are not wrapped.
use std::ops::Range;

/// The gutter is as wide as the widest number it can show plus one cell, so the text never shifts as it scrolls.
pub fn gutter_cells(shown: usize) -> usize {
    shown.to_string().len().max(2) + 1
}

/// The text starts this many cells after the gutter.
pub const TEXT_LEAD: usize = 2;

/// The rows a line wraps into at `cells` per row, as char ranges: a row ends after its last space when one is in
/// reach, else at the cell limit (Text.WrapAtWordBoundaryOrAnywhere). An empty line is one empty row.
pub fn wrap(text: &str, cells: usize) -> Vec<Range<usize>> {
    let chars: Vec<char> = text.chars().collect();
    let cells = cells.max(1);
    let mut rows = Vec::new();
    let mut start = 0;
    while chars.len() - start > cells {
        let limit = start + cells;
        let end = if chars[limit] == ' ' {
            limit + 1
        } else {
            chars[start..limit]
                .iter()
                .rposition(|c| *c == ' ')
                .map_or(limit, |at| start + at + 1)
        };
        rows.push(start..end);
        start = end;
    }
    rows.push(start..chars.len());
    rows
}

/// Where an unwrapped line is cut to end in an ellipsis inside `cells`, None when it fits.
pub fn elide(text: &str, cells: usize) -> Option<usize> {
    (text.chars().count() > cells).then(|| cells.saturating_sub(1))
}

/// The char range of each run inside one row's range, with the run's index, for painting a wrapped or cut row.
pub fn runs_in(lengths: &[usize], row: Range<usize>) -> Vec<(usize, Range<usize>)> {
    let mut out = Vec::new();
    let mut at = 0;
    for (index, len) in lengths.iter().enumerate() {
        let run = at..at + len;
        let (from, to) = (run.start.max(row.start), run.end.min(row.end));
        if from < to {
            out.push((index, from - at..to - at));
        }
        at += len;
    }
    out
}

#[cfg(test)]
#[path = "code_text_tests.rs"]
mod tests;
