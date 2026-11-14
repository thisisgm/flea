//! Header autofit: the text each held row draws in a column and the width that fits the widest.
use super::super::{
    columns::{self, Column},
    pane::Held,
    ListView,
};
use crate::row::{format, is_symlink};

/// ColumnFit.cellText(): the string a row draws in one column, measured by an autofit.
pub fn cell_text(column: Column, held: &Held, index: usize, view: &ListView) -> Option<String> {
    if column == Column::Mode && view.place_recent.is_some() {
        return None;
    }
    let facts = held.row(index)?;
    // Before the catalog words a date and a link's size word print nothing, so a fit there has nothing to measure.
    let worded = column == Column::Date || (column == Column::Size && is_symlink(facts.mode));
    if worded && !view.words.ready {
        return None;
    }
    Some(match column {
        Column::Mode => format::permissions(facts.mode),
        Column::Size => {
            let walked = if facts.dir {
                view.walked_for(held.listing, index)
            } else {
                None
            };
            format::size_text(facts, walked, &view.words)
        }
        Column::Date => view.date_stamp(facts).map_or_else(
            || "--".into(),
            |stamp| format::date_text(stamp, &view.words),
        ),
        // ColumnFit.cellText: the Kind name the row paints, so an autofit measures it.
        Column::Kind => facts.kind.clone(),
    })
}

/// Header.fittedWidth(): the widest held row's cell in the caption face, or None with no held row, which writes nothing.
pub fn fitted_width(
    column: Column,
    held: &Held,
    view: &ListView,
    current: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Option<u32> {
    let widths: Vec<f32> = (0..held.rows.len())
        .map(|at| held.index_at(at))
        .filter_map(|index| cell_text(column, held, index, view))
        .map(|text| measure(&text).ceil())
        .collect();
    (!widths.is_empty()).then(|| columns::autofit_width(&widths, f64::from(current)))
}
