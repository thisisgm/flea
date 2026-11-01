use super::{page_items, Move, Pane};

impl Pane {
    pub fn row_height(&self) -> f32 {
        self.row_height
    }

    pub fn viewport_height(&self) -> f32 {
        self.height
    }

    pub fn set_row_height(&mut self, row: f32) {
        if !row.is_finite() || row <= 0. || row == self.row_height {
            return;
        }
        // List/Dual publish rows plus header; Grid owns its separate scroll geometry.
        if let Some(extent) = &mut self.scroll_extent {
            *extent += self.held.total as f32 * (row - self.row_height);
        }
        self.scroll_y = self.scroll_y / self.row_height * row;
        self.row_height = row;
        self.scroll_by(0.);
    }

    pub fn wanted_window(&mut self) -> Option<(usize, usize)> {
        if self.swap.pending.is_some() {
            return None;
        }
        let first = (self.scroll_y / self.row_height).floor() as usize;
        self.requests
            .wanted(&self.held, first, self.visible_rows())
            .map(|request| (request.start, request.count))
    }
    fn max_scroll(&self) -> f32 {
        (self
            .scroll_extent
            .unwrap_or(self.held.total as f32 * self.row_height)
            - self.height)
            .max(0.)
    }
    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll_y = (self.scroll_y + delta).clamp(0., self.max_scroll());
    }
    pub fn scroll_to(&mut self, y: f32) {
        self.scroll_y = y.clamp(0., self.max_scroll());
    }
    pub fn move_cursor(&mut self, step: Move) -> bool {
        let total = self.held.total;
        if self.busy() || total == 0 {
            return false;
        }
        let page = page_items(self.visible_rows());
        let last = total - 1;
        self.cursor = match step {
            Move::Up => self.cursor.saturating_sub(1),
            Move::Down => (self.cursor + 1).min(last),
            Move::PageUp => self.cursor.saturating_sub(page),
            Move::PageDown => (self.cursor + page).min(last),
            Move::Home => 0,
            Move::End => last,
        };
        self.reveal_cursor();
        true
    }
    /// A filtered window: the listing run the drawn rows on screen span, cut to a few windows when matches lie far apart.
    pub fn wanted_filtered_window(&mut self, shown: &[usize]) -> Option<(usize, usize)> {
        if self.swap.pending.is_some() || shown.is_empty() {
            return None;
        }
        let visible = self.visible_rows();
        let first = ((self.scroll_y / self.row_height).floor() as usize).min(shown.len() - 1);
        let last = (first + visible).min(shown.len() - 1);
        let drawn = &shown[first..=last];
        let (lo, hi) = (*drawn.iter().min()?, *drawn.iter().max()?);
        let span = (hi - lo + 1).min(4 * super::window_size(visible));
        // Only the capped run is owed, so a held capped answer never asks the same window again.
        if lo >= self.held.start && lo + span <= self.held.start + self.held.rows.len() {
            return None;
        }
        self.requests
            .wanted(&self.held, lo, span)
            .map(|request| (request.start, request.count))
    }
    /// Scroll so a view position is on screen, the filter's showRow.
    pub fn reveal_view(&mut self, view: usize) {
        let top = view as f32 * self.row_height;
        if top < self.scroll_y {
            self.scroll_to(top);
        } else if top + self.row_height > self.scroll_y + self.height {
            self.scroll_to(top + self.row_height - self.height);
        }
    }
    pub fn reveal_cursor(&mut self) {
        let top = self.cursor as f32 * self.row_height;
        if top < self.scroll_y {
            self.scroll_to(top);
        } else if top + self.row_height > self.scroll_y + self.height {
            self.scroll_to(top + self.row_height - self.height);
        }
    }
    /// ScrollOff.cursorY at a context above 0: a landing keeps that many rows around the cursor, as v0.3.12's setCursor does.
    pub fn reveal_cursor_keeping(&mut self, context: usize) {
        let frame = crate::input::Frame {
            bounds: gpui::Bounds::new(
                gpui::point(gpui::px(0.), gpui::px(0.)),
                gpui::size(gpui::px(1.), gpui::px(self.height)),
            ),
            row: self.row_height,
            lane: 0.,
            content: self.max_scroll() + self.height,
            scroll: self.scroll_y,
            listing: self.held.listing,
            total: self.held.total,
            columns: 1,
            cell_width: 1.,
            grow: super::Grow::default(),
            cell_origin: (0., 0.),
        };
        self.scroll_to(crate::input::scroll::reveal(frame, self.cursor, context));
    }
}

#[cfg(test)]
#[path = "pane_geometry_tests.rs"]
mod tests;
