//! Picker hits reuse the listing's retained frame and the grid's painted tiles.
use super::view::ListingView;
use gpui::{Pixels, Point};

pub(crate) struct Hit {
    pub index: usize,
    pub directory: bool,
    pub checkbox: bool,
    pub path: String,
    pub first_path: String,
}

impl ListingView {
    pub(crate) fn pointer_hit(&mut self, position: Point<Pixels>) -> Option<Hit> {
        if self.listing().is_pending() {
            return None;
        }
        let held = self.listing().held();
        let (index, checkbox) = if self.view_mode() == "grid" {
            let state = self.surface.state.borrow();
            if !state
                .frame_held
                .as_ref()
                .is_some_and(|painted| std::rc::Rc::ptr_eq(painted, &held))
            {
                return None;
            }
            if !state.bounds.contains(&position) {
                return None;
            }
            let layout = state.layout?;
            let x = f32::from(position.x - state.bounds.left());
            let y = f32::from(position.y - state.bounds.top());
            let at = layout.index_at(state.scroll, x, y)?;
            let tile = state.drawn().iter().find(|tile| tile.view == at)?;
            let cell = layout.cell(state.scroll, at);
            // The mark's square with its gap on each side, at the cell's top left where surface.rs draws it.
            let reach = 2. * layout.metrics.gap
                + crate::list::picker::check_size(
                    layout.metrics.body_small,
                    layout.metrics.hairline,
                );
            let check = self.surface.marks_allowed && x <= cell.x + reach && y <= cell.y + reach;
            if !check && state.tile_at(x, y) != Some(at) {
                return None;
            }
            (tile.listing, check)
        } else {
            let state = self.state.borrow();
            let drawn = &state.drawn;
            if drawn.listing != held.listing {
                return None;
            }
            let frame = crate::input::Frame {
                bounds: drawn.mask,
                row: drawn.metrics.row,
                lane: drawn.metrics.lane,
                content: 0.,
                scroll: drawn.scroll_y,
                listing: drawn.listing,
                total: state.visible().last().map_or(0, |row| row.view + 1),
                columns: 1,
                cell_width: f32::from(drawn.mask.size.width),
                grow: Default::default(),
                cell_origin: (0., 0.),
            };
            let at = frame.row_at(position)?;
            let index = state
                .visible()
                .iter()
                .find(|row| row.view == at && row.prepared.is_some())?
                .index;
            let check = self.surface.marks_allowed
                && f32::from(position.x - drawn.mask.left())
                    <= drawn.metrics.pad + drawn.metrics.slot + drawn.metrics.gap;
            (index, check)
        };
        let row = held.rows.get(index.checked_sub(held.start)?)?;
        let directory = flea::ui_logic_picker_listing::directory(row.dir, row.mode, &row.icon);
        let checkbox = checkbox && directory == self.state.borrow().view.picker_directory;
        let path =
            flea::ui_logic_picker_listing::row_path(&self.listing().facts().location, &row.name);
        let first_path = std::mem::replace(&mut self.last_tap, path.clone());
        Some(Hit {
            index,
            directory,
            checkbox,
            path,
            first_path,
        })
    }
}
