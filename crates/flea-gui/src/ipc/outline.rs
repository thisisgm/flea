//! Outline observations read the frame the retained List actually painted.
use crate::list::ListState;
use flea::protocol::RowFacts;

pub(super) fn state(list: &ListState) -> String {
    format!(
        "{{\"listing\":{},\"total\":{},\"loaded\":{},\"rows\":{},\"rowsBuilt\":{}}}",
        list.drawn.listing,
        list.drawn_rows.total,
        list.drawn_rows.rows.len(),
        list.visible().len(),
        list.rows.misses
    )
}

pub(super) fn row(function: &str, row: Option<&RowFacts>) -> Option<String> {
    Some(match function {
        "rowDepth" => row.map(|row| row.depth.to_string()).unwrap_or_default(),
        "rowExpanded" => row.map(|row| row.open.to_string()).unwrap_or_default(),
        "rowRelativePath" => row.map(|row| row.name.clone()).unwrap_or_default(),
        _ => return None,
    })
}
