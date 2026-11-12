//! One row's cells as ui/Row.qml and ui/js/Format.js print them, built only when a row enters the viewport.
pub mod cache;
mod elide;
#[cfg(test)]
pub(crate) mod fit_tests;
pub mod format;
/// Text.ElideLeft for Recent's fixed Location cell: keep the parent path's tail.
pub fn left_elide(text: &str, max_cells: usize) -> Cow<'_, str> {
    if text.len() <= max_cells || units(text).map(|unit| unit.1).sum::<usize>() <= max_cells {
        return text.into();
    }
    let budget = max_cells.saturating_sub(1);
    let mut used = 0;
    let mut start = text.len();
    for (at, cells) in units(text).collect::<Vec<_>>().into_iter().rev() {
        if used + cells > budget {
            break;
        }
        used += cells;
        start = at;
    }
    format!("\u{2026}{}", &text[start..]).into()
}

pub(crate) mod language_capture;
#[cfg(test)]
mod language_tests;
pub mod marks;
pub mod tags;

use flea::protocol::RowFacts;
use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;

pub use elide::{elide_around, letters_drawn};

const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;
const ANY_EXECUTE_BIT: u32 = 0o111;

/// Which palette role draws a name at rest; a lifted row draws every name in the foreground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameInk {
    Foreground,
    Symlink,
    Executable,
}

pub fn is_symlink(mode: u32) -> bool {
    mode & S_IFMT == S_IFLNK
}

/// Row.qml nameColor(): a link, then a folder, then an executable file the mount reports real modes for.
pub fn name_ink(row: &RowFacts) -> NameInk {
    if is_symlink(row.mode) {
        NameInk::Symlink
    } else if row.dir {
        NameInk::Foreground
    } else if row.real_modes && row.mode & ANY_EXECUTE_BIT != 0 {
        NameInk::Executable
    } else {
        NameInk::Foreground
    }
}

/// Row.qml decoratedName: the name, then a link's target, each as shown_name draws it.
pub fn decorated_name(row: &RowFacts) -> String {
    match &row.link {
        Some(target) => format!("{} -> {}", shown_name(&row.name), shown_name(target)),
        None => shown_name(&row.name).into_owned(),
    }
}

/// A name as drawn: raw name bytes read in their lossy spelling, and bidi embedding, override and isolate controls are
/// dropped, so it never reorders later text.
pub fn shown_name(name: &str) -> Cow<'_, str> {
    let name = flea::bytename::display(name);
    let control = |c: char| matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}');
    if name.contains(control) {
        name.chars()
            .filter(|&c| !control(c))
            .collect::<String>()
            .into()
    } else {
        name
    }
}

const WIDE: [(u32, u32); 16] = [
    (0x1100, 0x115F),
    (0x2E80, 0x303E),
    (0x3041, 0x33FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0xA4CF),
    (0xAC00, 0xD7A3),
    (0xF900, 0xFAFF),
    (0xFE10, 0xFE19),
    (0xFE30, 0xFE4F),
    (0xFF00, 0xFF60),
    (0xFFE0, 0xFFE6),
    (0x20000, 0x3FFFD),
    (0x2600, 0x26FF),
    (0x2700, 0x27BF),
    (0x2B00, 0x2BFF),
    (0x1F000, 0x1FAFF),
];

/// Format.js cellWidthOf(): East Asian wide, fullwidth and emoji paint two cells, all else one.
pub fn cells_of(ch: char) -> usize {
    let code = ch as u32;
    if code >= 0x1100 && WIDE.iter().any(|&(low, high)| (low..=high).contains(&code)) {
        2
    } else {
        1
    }
}

/// Format.js charsOf() cells, cut only between grapheme clusters, so a Thai vowel or a Devanagari sign is never cut from its letter.
fn units(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    text.grapheme_indices(true)
        .map(|(at, unit)| (at, unit_cells(unit)))
}

/// A cluster's cells as Format.js counts its chars: a U+0300 to U+036F mark rides free, every other char keeps its cells.
pub(crate) fn unit_cells(unit: &str) -> usize {
    let cells: usize = unit
        .chars()
        .filter(|ch| !('\u{300}'..='\u{36f}').contains(ch))
        .map(cells_of)
        .sum();
    cells.max(1)
}

/// A text's Format.js cells, summed over its units.
pub fn cell_count(text: &str) -> usize {
    units(text).map(|unit| unit.1).sum()
}

/// The largest cell budget whose elision `fits` once shaped, searched from the cell recipe's own budget, so glyph widths decide where the cells only guess.
pub fn fit_budget(budget: usize, total: usize, fits: impl Fn(usize) -> bool) -> usize {
    let budget = budget.min(total);
    // The whole name draws no ellipsis, so it can fit where one cell less does not.
    if budget < total && fits(total) {
        return total;
    }
    let (mut low, mut high) = if fits(budget) {
        if budget == total || !fits(budget + 1) {
            return budget;
        }
        (budget + 1, total)
    } else if budget == 0 {
        return 0;
    } else {
        (0, budget - 1)
    };
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if fits(mid) {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

// Shaped advances sum in f32, so a name exactly its cells wide may read a hair over them.
pub(crate) const FIT_SLACK: f32 = 0.01;

/// A name elided by `elide` at the largest budget whose shaped `width` fits the cells' pixels, so a CJK name is not cut early and a wide script never runs past its cells; no width or advance keeps the cell budget.
pub fn fitted<'a, T>(
    name: &str,
    budget: usize,
    advance: f32,
    width: Option<&dyn Fn(&str) -> f32>,
    elide: impl Fn(usize) -> (Cow<'a, str>, T),
) -> (Cow<'a, str>, T) {
    let room = budget as f32 * advance;
    let budget = match width.filter(|_| advance > 0.) {
        Some(width) => fit_budget(budget, cell_count(name), |cells| {
            width(&elide(cells).0) <= room + FIT_SLACK
        }),
        None => budget,
    };
    elide(budget)
}

/// Format.middleElide(): the head takes the odd cell, so a 49-cell budget keeps 24 and 24.
pub fn middle_elide(text: &str, max_cells: usize) -> Cow<'_, str> {
    // A char never paints more cells than its UTF-8 bytes, so a short name fits without a scan.
    if text.len() <= max_cells || units(text).map(|unit| unit.1).sum::<usize>() <= max_cells {
        return text.into();
    }
    let units: Vec<(usize, usize)> = units(text).collect();
    let head_budget = max_cells.saturating_sub(1).div_ceil(2);
    let tail_budget = max_cells.saturating_sub(1) / 2;
    let (mut used, mut head) = (0, 0);
    while head < units.len() && used + units[head].1 <= head_budget {
        used += units[head].1;
        head += 1;
    }
    let (mut used, mut tail) = (0, units.len());
    while tail > head && used + units[tail - 1].1 <= tail_budget {
        used += units[tail - 1].1;
        tail -= 1;
    }
    let head_end = units.get(head).map_or(text.len(), |unit| unit.0);
    let tail_start = units.get(tail).map_or(text.len(), |unit| unit.0);
    format!("{}\u{2026}{}", &text[..head_end], &text[tail_start..]).into()
}

/// MatchText's ElideRight for a name holding a match run: the head keeps every cell it can and the tail becomes one ellipsis.
pub fn right_elide(text: &str, max_cells: usize) -> Cow<'_, str> {
    match cut(text, max_cells) {
        Some((end, _)) => format!("{}\u{2026}", &text[..end]).into(),
        None => text.into(),
    }
}

/// ElideRight's cut byte and preceding cells, stopping at overflow; None when the whole text fits.
fn cut(text: &str, max_cells: usize) -> Option<(usize, usize)> {
    if text.len() <= max_cells {
        return None;
    }
    let budget = max_cells.saturating_sub(1);
    let (mut used, mut found) = (0, None);
    for (at, cells) in units(text) {
        if found.is_none() && used + cells > budget {
            found = Some((at, used));
        }
        used += cells;
        if used > max_cells {
            return found;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // A raw name that is not UTF-8 and holds an override draws in its lossy spelling without the control.
    #[test]
    fn a_raw_override_name_draws_lossily_without_its_control() {
        let raw = flea::bytename::name_text(b"\xe2\x80\xaecol\xff.txt");
        assert_eq!(shown_name(&raw), "col\u{fffd}.txt");
        assert_eq!(shown_name("plain.txt"), "plain.txt");
    }

    fn row(name: &str, mode: u32) -> RowFacts {
        RowFacts {
            name: name.into(),
            mode,
            real_modes: true,
            ..RowFacts::default()
        }
    }

    #[test]
    fn name_ink_follows_row_qml_order() {
        assert_eq!(name_ink(&row("a", 0o120777)), NameInk::Symlink);
        let folder = RowFacts {
            dir: true,
            ..row("d", 0o040755)
        };
        assert_eq!(name_ink(&folder), NameInk::Foreground);
        assert_eq!(name_ink(&row("x", 0o100755)), NameInk::Executable);
        let unreal = RowFacts {
            real_modes: false,
            ..row("x", 0o100755)
        };
        assert_eq!(name_ink(&unreal), NameInk::Foreground);
        let link = RowFacts {
            link: Some("/usr/share/omarchy".into()),
            ..row("shell", 0o120777)
        };
        assert_eq!(decorated_name(&link), "shell -> /usr/share/omarchy");
        let spoof = RowFacts {
            link: Some("/tmp/\u{2067}evil".into()),
            ..row("\u{202e}txt.exe", 0o120777)
        };
        assert_eq!(decorated_name(&spoof), "txt.exe -> /tmp/evil");
    }

    #[test]
    fn middle_elide_matches_format_js() {
        let long = "screenshot-2026-08-30-final-review-for-gm-after-the-bench-v3.png";
        assert_eq!(
            middle_elide(long, 49),
            "screenshot-2026-08-30-fi\u{2026}m-after-the-bench-v3.png"
        );
        assert_eq!(middle_elide("iiiiiiiiii.txt", 7), "iii\u{2026}txt");
        assert_eq!(middle_elide("short.txt", 49), "short.txt");
        assert_eq!(middle_elide("abcdef", 0), "\u{2026}");
        // Wide chars count two cells, so a 5-cell budget keeps one head and one tail char.
        assert_eq!(
            middle_elide("\u{4f60}\u{597d}\u{754c}\u{9762}", 5),
            "\u{4f60}\u{2026}\u{9762}"
        );
        // A combining accent stays on its base on either side of the cut.
        assert_eq!(
            middle_elide("e\u{301}abcde\u{301}", 3),
            "e\u{301}\u{2026}e\u{301}"
        );
        assert_eq!(
            middle_elide("e\u{301}\u{301}\u{301}ab", 3),
            "e\u{301}\u{301}\u{301}ab"
        );
        assert_eq!(
            middle_elide("a\u{1f389}b\u{1f389}c\u{1f389}d", 5),
            "a\u{2026}d"
        );
    }

    #[test]
    fn right_elide_keeps_the_head_for_a_matched_name() {
        assert_eq!(
            right_elide("notes-from-the-meeting.txt", 10),
            "notes-fro\u{2026}"
        );
        assert_eq!(right_elide("notes.txt", 10), "notes.txt");
        assert_eq!(
            right_elide("\u{4f60}\u{597d}\u{754c}\u{9762}", 6),
            "\u{4f60}\u{597d}\u{2026}"
        );
        assert_eq!(
            right_elide("e\u{301}e\u{301}e\u{301}e\u{301}", 3),
            "e\u{301}e\u{301}\u{2026}"
        );
        assert_eq!(right_elide("abc", 0), "\u{2026}");
    }
    #[test]
    fn left_elide_keeps_path_tails_and_whole_character_units() {
        assert_eq!(
            left_elide("/home/user/work/StageDocs", 12),
            "\u{2026}k/StageDocs"
        );
        assert_eq!(left_elide("short", 5), "short");
        assert_eq!(left_elide("abc", 0), "\u{2026}");
        assert_eq!(
            left_elide("\u{4f60}\u{597d}\u{754c}\u{9762}", 5),
            "\u{2026}\u{754c}\u{9762}"
        );
        assert_eq!(left_elide("ae\u{301}be\u{301}", 3), "\u{2026}be\u{301}");
    }
}
