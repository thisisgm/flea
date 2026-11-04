//! Header taps: how far a press may travel and which title sort key lies under a point.
use super::HeaderState;
use crate::list::{
    columns::{Column, Layout},
    Metrics,
};

// Qt's startDragDistance: a press that travels farther is a drag, never a tap (Header.qml's TapHandler).
pub const TAP_SLOP: f32 = 10.;

/// The sort key of the title under x, empty over the mode column and the padding (in Dual also over the mark slot and gap).
pub fn title_key(layout: &Layout, metrics: &Metrics, x: f32, dual: bool) -> &'static str {
    Column::ALL
        .into_iter()
        .find(|column| {
            layout
                .cell(*column)
                .is_some_and(|cell| x >= cell.x && x < cell.right())
        })
        .map(|column| match column {
            Column::Mode => "",
            Column::Size => "size",
            Column::Date => "mtime",
            Column::Kind => "kind",
        })
        .unwrap_or_else(|| {
            HeaderState::title_cell(layout, metrics, None, dual)
                .filter(|cell| x >= cell.x && x < cell.right())
                .map_or("", |_| "name")
        })
}
