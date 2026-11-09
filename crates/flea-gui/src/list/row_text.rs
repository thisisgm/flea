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
    let display = super::outline::display(facts);
    let name = row::decorated_name(&display);
    let budget = if extra.clipped {
        shaping.clip_cells
    } else {
        shaping.name_cells
    };
    let budget = extra.drop.map_or(budget, |drop| drop.min(budget));
    let run = format::match_run(&row::shown_name(&display.name), query);
    let elide = |cells: usize| match run {
        None => (row::middle_elide(&name, cells), None),
        Some(run) => {
            let (drawn, (start, end)) = row::elide_around(&name, cells, run);
            (drawn, (start < end).then_some((start, end)))
        }
    };
    let (drawn, run) = row::fitted(&name, budget, shaping.advance, width, elide);
    (drawn.into_owned(), run)
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
    let (name, run) = drawn_name(&display, extra, &name_shaping, &view.query, Some(&width));
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
        bold: OnceCell::new(),
    }
}

/// The cursor row's name with its match run in bold, the board's cursor treatment for a marked name.
pub(super) fn bold_name(prepared: &Prepared, shaping: &Shaping, window: &Window) -> GlyphRun {
    let runs =
        crate::g2::file_search::run::text_runs(&prepared.label, prepared.run, true, &shaping.font);
    GlyphRun::new(
        window
            .text_system()
            .layout_line(&prepared.label, shaping.body, &runs, None),
    )
}
