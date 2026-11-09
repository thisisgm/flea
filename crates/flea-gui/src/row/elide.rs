//! MatchText's elision around a match run, so the run stays drawn whole inside the name's cells.
use super::{cell_count, cut};
use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;

#[cfg(test)]
#[path = "elide_tests.rs"]
mod tests;

const ELLIPSIS: char = '\u{2026}';

/// The start of the grapheme cluster holding byte `at`.
fn cluster_start(text: &str, at: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(start, _)| start)
        .take_while(|start| *start <= at)
        .last()
        .unwrap_or(0)
}

/// The end of the grapheme cluster holding the byte before `at`, for `at` above 0.
fn cluster_end(text: &str, at: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(start, unit)| start + unit.len())
        .find(|end| *end >= at)
        .unwrap_or(text.len())
}

/// Pushes `text` with ElideRight to `max_cells` and returns how many of its bytes stay.
fn push_elided(drawn: &mut String, text: &str, max_cells: usize) -> usize {
    match cut(text, max_cells) {
        Some((at, _)) => {
            drawn.push_str(&text[..at]);
            drawn.push(ELLIPSIS);
            at
        }
        None => {
            drawn.push_str(text);
            text.len()
        }
    }
}

/// Reserve the match run's cells before eliding its head and tail, returning its drawn byte range.
pub fn elide_around(
    text: &str,
    max_cells: usize,
    run: (usize, usize),
) -> (Cow<'_, str>, (usize, usize)) {
    // A run that ends inside a cluster, a Thai base matched without its vowel, takes the whole cluster; an empty run stays empty.
    let run = if run.0 < run.1 {
        (cluster_start(text, run.0), cluster_end(text, run.1))
    } else {
        run
    };
    let (start, end) = run;
    let Some((kept, _)) = cut(text, max_cells) else {
        return (text.into(), run);
    };
    if end <= kept || start == 0 || max_cells < 2 {
        let drawn = format!("{}{ELLIPSIS}", &text[..kept]);
        return (drawn.into(), (start.min(kept), end.min(kept)));
    }
    let mut drawn = String::with_capacity(text.len() + 2 * ELLIPSIS.len_utf8());
    let room = max_cells.saturating_sub(cell_count(&text[start..end]));
    if room == 0 {
        drawn.push(ELLIPSIS);
        let shown = push_elided(&mut drawn, &text[start..], max_cells - 1);
        let at = ELLIPSIS.len_utf8();
        return (drawn.into(), (at, at + (end - start).min(shown)));
    }
    let (before, after) = (&text[..start], &text[end..]);
    // A whole head that fills the room gives one cell back, so a cut tail still shows its ellipsis.
    let head = cut(before, room).or_else(|| {
        if after.is_empty() {
            None
        } else {
            cut(before, room - 1)
        }
    });
    let rest = match head {
        Some((at, used)) => {
            drawn.push_str(&before[..at]);
            drawn.push(ELLIPSIS);
            room - used - 1
        }
        None => {
            drawn.push_str(before);
            room - cell_count(before)
        }
    };
    let at = drawn.len();
    drawn.push_str(&text[start..end]);
    if rest > 0 {
        push_elided(&mut drawn, after, rest);
    }
    (drawn.into(), (at, at + end - start))
}

/// Each matched stretch inside a run carried to where `elide_around` drew that run, clipped to it; `text` is the name the run was found in.
pub fn letters_drawn(
    text: &str,
    letters: &[(usize, usize)],
    drawn: (usize, usize),
) -> Vec<(usize, usize)> {
    let Some(&(first, _)) = letters.first() else {
        return Vec::new();
    };
    // The drawn run opens where its first letter's cluster opened in the name.
    let from = cluster_start(text, first);
    letters
        .iter()
        .map(|&(start, end)| {
            let start = (drawn.0 + start.saturating_sub(from)).min(drawn.1);
            (start, (drawn.0 + end.saturating_sub(from)).min(drawn.1))
        })
        .filter(|(start, end)| start < end)
        .collect()
}
