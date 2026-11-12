//! A newly grown refusal row uses its view's ordinary minimal cursor reveal.
use super::Shell;
use crate::{columns::geometry::Slot, input::Frame, list::pane::Grow};

impl Shell {
    pub(crate) fn reveal_rename_grid(&mut self, previous: [f32; 2]) {
        for (side, before) in previous.into_iter().enumerate() {
            if self.bands[side] <= 0. || self.bands[side] == before {
                continue;
            }
            let Some(index) = self.renaming_row(side) else {
                continue;
            };
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            let position = host.input.view_index(index);
            let Some(grid) = host.grid.as_mut() else {
                continue;
            };
            let shown = host.filter.shown.as_deref().map_or(
                crate::grid::Shown::All(host.pane.held.total),
                crate::grid::Shown::Only,
            );
            grid.view.scroll = grid.view.layout(shown).reveal(grid.view.scroll, position);
            grid.view.glide = None;
        }
    }

    pub(crate) fn reveal_rename_column(&mut self, previous: [Grow; 2]) {
        for (side, before) in previous.into_iter().enumerate() {
            let grow = self.grows[side];
            if grow.band <= 0. || grow == before {
                continue;
            }
            let neighbour = self.rename_neighbour_row(side);
            let Some(host) = self.panes[side].as_mut() else {
                continue;
            };
            if let Some((slot, index, _)) = neighbour {
                let Some(columns) = host.columns.as_mut() else {
                    continue;
                };
                let Some(paint) = columns.painters.get(slot) else {
                    continue;
                };
                let paint = paint.borrow();
                let Some(drawn) = paint.drawn.as_ref() else {
                    continue;
                };
                let Some(position) = drawn.position_of(index) else {
                    continue;
                };
                let grow = Grow {
                    at: position,
                    band: grow.band,
                };
                let total = match &drawn.tracks[drawn.at].rows {
                    crate::columns::TrackRows::Peek(rows) => rows.len(),
                    _ => continue,
                };
                let frame = Frame {
                    bounds: drawn.bounds,
                    row: drawn.row_height,
                    content: grow.content(total, drawn.row_height)
                        + crate::columns::geometry::FOOTER,
                    scroll: columns.area.scroll(slot),
                    total,
                    grow,
                    lane: 0.,
                    listing: host.pane.held.listing,
                    columns: 1,
                    cell_width: f32::from(drawn.bounds.size.width),
                    cell_origin: (0., 0.),
                };
                let to = crate::input::scroll::reveal(frame, position, 0);
                columns
                    .area
                    .scroll_grown(slot, to - frame.scroll, grow.band);
                columns.scroll[slot.index()].input.halt();
            } else if let Some(frame) = host.columns_key_frame(grow) {
                host.pane.scroll_extent = Some(frame.content);
                host.pane
                    .scroll_to(crate::input::scroll::reveal(frame, grow.at, 0));
                if let Some(columns) = host.columns.as_mut() {
                    columns.scroll[Slot::Active.index()].input.halt();
                }
            }
        }
    }
}
