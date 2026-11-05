//! The folder-wide tag slot between Mode and Size, shared by every row in a tagged listing.
use super::{
    columns::{Cell, Column, Layout},
    pane::Held,
    ListState, Metrics,
};
use crate::palette::Theme;
use flea::ui_logic_pl4::tags::{ROW_SLOT, SWATCH_SIZE};
use gpui::{point, px, Bounds, Pixels, Window};

pub(super) fn reserve(mut layout: Layout, shown: bool, gap: f32) -> Layout {
    if shown {
        let width = (ROW_SLOT + gap).min(layout.name.width);
        layout.name.width -= width;
        // Beside the name Mode moves left with it; anywhere else the shrunken name alone makes the room.
        if layout.mode_leads() {
            if let Some(mode) = &mut layout.cells[Column::Mode.slot()] {
                mode.x -= width;
            }
        }
    }
    layout
}

fn slot(layout: &Layout, metrics: &Metrics, row_width: f32) -> Cell {
    // Mode sits beside the name only when it is the leftmost drawn column; otherwise the slot follows the name.
    let mode = layout.cell(Column::Mode).filter(|_| layout.mode_leads());
    let left = mode.unwrap_or(layout.name).right() + metrics.gap;
    let right = Column::ALL
        .into_iter()
        .filter(|column| *column != Column::Mode)
        .filter_map(|column| layout.cell(column))
        .map(|cell| cell.x - metrics.gap)
        .fold(row_width - metrics.pad, f32::min);
    Cell {
        x: left,
        width: (right - left).clamp(0., ROW_SLOT),
    }
}

pub(super) fn paint(
    state: &ListState,
    held: &Held,
    bounds: Bounds<Pixels>,
    theme: &Theme,
    window: &mut Window,
) {
    if !state.view.tags_shown {
        return;
    }
    let drawn = &state.drawn;
    let metrics = &drawn.metrics;
    let slot = slot(
        &drawn.layout,
        metrics,
        f32::from(bounds.size.width) - metrics.lane,
    );
    for shown in &state.visible {
        let Some(tags) = held.row(shown.index).and_then(|row| row.tags.as_deref()) else {
            continue;
        };
        let top = bounds.origin.y + px(shown.view as f32 * metrics.row - drawn.scroll_y);
        crate::row::tags::paint(
            tags,
            point(
                bounds.origin.x + px(slot.x),
                top + px((metrics.row - SWATCH_SIZE) / 2.),
            ),
            slot.width,
            &theme.tag_listing_inks,
            theme.foreground,
            window,
        );
    }
}
