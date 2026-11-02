//! Header resize handles follow the drawn set; Recent Location stays fixed.
use super::{Column, Layout, ListView, Metrics};

/// One grab zone over a drawn column's left edge: ResizeHandle.qml's nine pixels, offset by half so the hairline sits in its middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub column: Column,
    pub x: f32,
    pub width: f32,
}

/// Header.qml's handles Loader: zones exist only in a single pane's list header with no search on, one per drawn column.
pub fn handles(
    layout: &Layout,
    metrics: &Metrics,
    view: &ListView,
) -> impl Iterator<Item = Handle> {
    let active = !view.dual && !view.searching;
    let width = metrics.handle;
    let half = (width / 2.).floor();
    let layout = *layout;
    Column::ALL.into_iter().filter_map(move |column| {
        let cell = layout
            .cell(column)
            .filter(|_| active && !(layout.recent_location && column == Column::Mode))?;
        Some(Handle {
            column,
            x: cell.x - half,
            width,
        })
    })
}

/// The zone under a header-local x, if any.
pub fn handle_at(layout: &Layout, metrics: &Metrics, view: &ListView, x: f32) -> Option<Column> {
    handles(layout, metrics, view)
        .find(|handle| x >= handle.x && x < handle.x + handle.width)
        .map(|handle| handle.column)
}
