//! Name elision and prepared text shared by the List Element's retained rows.
use super::{
    columns::{Column, Layout},
    ListView,
};
use crate::row::{
    self,
    cache::{Extra, Prepared, Shaping},
    format,
    marks::Mark,
};
use flea::protocol::RowFacts;
use gpui::{Font, GlyphRun, Hsla, LineLayout, Pixels, TextRun, Window};
use std::{borrow::Cow, cell::OnceCell, sync::Arc};

pub(super) fn run_of(len: usize, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

/// The name as drawn: middle-elided, or elided around a match run kept whole, at the budget its shaped `width` fits in the cells' pixels when one is given.
pub fn drawn_name(
    facts: &RowFacts,
    extra: Extra,
    shaping: &Shaping,
    query: &str,
    width: Option<&dyn Fn(&str) -> f32>,
) -> (String, Option<(usize, usize)>) {
    let (drawn, run, _) = drawn_letters(facts, extra, shaping, query, width);
    (drawn, run)
}

/// A byte range in a drawn name: a match run, or one matched stretch inside it.
pub type Span = (usize, usize);

/// `drawn_name` with each matched stretch of its run in the drawn name, so a close match washes its letters and never the gaps.
pub fn drawn_letters(
    facts: &RowFacts,
    extra: Extra,
    shaping: &Shaping,
    query: &str,
    width: Option<&dyn Fn(&str) -> f32>,
) -> (String, Option<Span>, Vec<Span>) {
    let display = super::outline::display(facts);
    let name = row::decorated_name(&display);
    let budget = if extra.clipped {
        shaping.clip_cells
    } else {
        shaping.name_cells
    };
    let budget = extra.drop.map_or(budget, |drop| drop.min(budget));
    let letters = format::match_letters(&display.name, query);
    let run = letters
        .first()
        .zip(letters.last())
        .map(|(first, last)| (first.0, last.1));
    let elide = |cells: usize| match run {
        None => (row::middle_elide(&name, cells), None),
        Some(run) => {
            let (drawn, (start, end)) = row::elide_around(&name, cells, run);
            (drawn, (start < end).then_some((start, end)))
        }
    };
    let (drawn, run) = row::fitted(&name, budget, shaping.advance, width, elide);
    let letters = run.map_or_else(Vec::new, |run| row::letters_drawn(&name, &letters, run));
    (drawn.into_owned(), run, letters)
}

/// Shapes one row's cells once, each elided to its own budget, so no long name is shaped whole and no hidden column at all.
pub(super) fn prepare(
    facts: &RowFacts,
    extra: Extra,
    shaping: &Shaping,
    view: &ListView,
    layout: &Layout,
    blank: &Arc<LineLayout>,
    window: &Window,
) -> Prepared {
    let text = window.text_system();
    let shape = |value: &str, size: Pixels| {
        GlyphRun::new(text.layout_line(value, size, &[run_of(value.len(), &shaping.font)], None))
    };
    let cell = |column: Column, value: &dyn Fn() -> String| {
        if layout.shown(column) {
            shape(
                &row::right_elide(&value(), shaping.cells[column.slot()]),
                shaping.caption,
            )
        } else {
            GlyphRun::new(Arc::clone(blank))
        }
    };
    let mut display = Cow::Borrowed(facts);
    let mut name_shaping = Cow::Borrowed(shaping);
    let location = if view.searching {
        display.to_mut().name = facts
            .name
            .rsplit('/')
            .next()
            .unwrap_or(&facts.name)
            .to_owned();
        let location = crate::g2::file_search::run::location(
            &facts.name,
            "",
            "",
            flea::ui_logic_file_search::Scope::Folder,
        );
        let location = row::right_elide(&location, shaping.name_cells / 2).into_owned();
        if !location.is_empty() {
            let reserved = location.chars().map(row::cells_of).sum::<usize>();
            let shaped = name_shaping.to_mut();
            shaped.name_cells = shaped.name_cells.saturating_sub(reserved + 1);
            shaped.clip_cells = shaped.name_cells;
        }
        location
    } else {
        if view.recent || view.place_recent.is_some() {
            display.to_mut().name = facts
                .name
                .rsplit('/')
                .next()
                .unwrap_or(&facts.name)
                .to_owned();
        }
        String::new()
    };
    let width = |value: &str| f32::from(shape(value, shaping.body).line().width);
    let (name, run, letters) =
        drawn_letters(&display, extra, &name_shaping, &view.query, Some(&width));
    let words = &view.words;
    let date = || {
        let full = view
            .date_stamp(facts)
            .map_or_else(|| "--".into(), |stamp| format::date_text(stamp, words));
        if view.picker.is_some() {
            full.split(' ').next().unwrap_or(&full).to_owned()
        } else {
            full
        }
    };
    Prepared {
        label: name.clone(),
        size_text: format::size_text(facts, extra.walked, words),
        date_text: date(),
        location: if location.is_empty() {
            GlyphRun::new(Arc::clone(blank))
        } else {
            shape(&location, shaping.caption)
        },
        name: shape(&name, shaping.body),
        size: cell(Column::Size, &|| {
            format::size_text(facts, extra.walked, words)
        }),
        date: cell(Column::Date, &date),
        mode: cell(Column::Mode, &|| {
            if view.place_recent.is_some() {
                row::left_elide(
                    &view.recent_location(&facts.name),
                    shaping.cells[Column::Mode.slot()],
                )
                .into_owned()
            } else {
                format::permissions(facts.mode)
            }
        }),
        // Row.kindText(): the reply's kinds[k], empty when the row names none.
        kind: cell(Column::Kind, &|| facts.kind.clone()),
        mark: Mark::for_facts(facts, shaping.languages),
        ink: row::name_ink(facts),
        run,
        letters,
        bold: OnceCell::new(),
    }
}

/// The cursor row's name with its match run in bold, the board's cursor treatment for a marked name.
pub(super) fn bold_name(prepared: &Prepared, shaping: &Shaping, window: &Window) -> GlyphRun {
    let bold = Font {
        weight: gpui::FontWeight::BOLD,
        ..shaping.font.clone()
    };
    // Each matched stretch in bold and every letter between them plain, as the wash marks them at rest.
    let mut runs = Vec::with_capacity(prepared.letters.len() * 2 + 1);
    let mut at = 0;
    for &(start, end) in &prepared.letters {
        let start = start.max(at);
        if end <= start {
            continue;
        }
        if start > at {
            runs.push(run_of(start - at, &shaping.font));
        }
        runs.push(run_of(end - start, &bold));
        at = end;
    }
    if prepared.label.len() > at {
        runs.push(run_of(prepared.label.len() - at, &shaping.font));
    }
    GlyphRun::new(
        window
            .text_system()
            .layout_line(&prepared.label, shaping.body, &runs, None),
    )
}
