//! Header words shared by shaping and its painted text receipt.
use crate::list::{columns::Column, ListView};

pub(super) const CELLS: [Option<Column>; 5] = [
    None,
    Some(Column::Mode),
    Some(Column::Size),
    Some(Column::Date),
    Some(Column::Kind),
];

pub(super) fn label(view: &ListView, column: Option<Column>) -> &str {
    let words = &view.words;
    match column {
        None => &words.name,
        Some(Column::Mode) if view.place_recent.is_some() => &words.location,
        Some(Column::Mode) => &words.mode,
        Some(Column::Size) => &words.size,
        Some(Column::Date) if view.place_recent.is_some() => &words.used,
        Some(Column::Date) => &words.modified,
        Some(Column::Kind) => &words.kind,
    }
}

/// Header.titles(): every title's plain word, whatever is drawn, as the header case reads them.
pub fn titles(view: &ListView) -> String {
    // Recent lays Kind out at 0 px, so its header reads the four columns it draws, as v0.3.12's did.
    let recent = view.place_recent.is_some();
    CELLS
        .into_iter()
        .filter(|column| !(recent && *column == Some(Column::Kind)))
        .map(|column| label(view, column))
        .collect::<Vec<_>>()
        .join("|")
}

pub(super) fn painted(view: &ListView, column: Option<Column>) -> String {
    let word = label(view, column);
    match super::sort_key(column) {
        Some(sorted) if sorted == view.order.key && !view.recent => {
            crate::navigation::sort::title(word, view.order.desc)
        }
        _ => word.to_owned(),
    }
}
