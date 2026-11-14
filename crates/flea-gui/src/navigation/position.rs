//! Where the cursor stood on screen in each folder a pane left, so coming back draws the list as it was.
//!
//! A landing on a remembered folder's remembered row scrolls so that row sits on the same screen line,
//! rather than at the edge a plain reveal leaves it. Each pane's navigator keeps its own, and only while
//! ui.json "keepScreenLine" is on for its window. Grid keeps its own scroll, so there the tile row's painted
//! offset is kept and handed to Grid's landing instead.
use super::anchor::Viewport;
use crate::pane::Pane;
use std::{collections::HashMap, rc::Rc};

const KEPT: usize = 512;

#[derive(Default)]
pub struct Lines {
    /// Folder path to the cursor row's name, its line on screen in rows from the top, and in Grid its tile row's
    /// painted offset.
    seen: HashMap<String, (String, f32, Option<f32>)>,
    /// The Grid's viewport while Grid draws the pane, refreshed each Grid frame; None in List and Columns.
    pub(crate) grid: Option<Viewport>,
    /// The listing rows a standing filter draws, in view order; the scroll counts these, not the listing.
    pub(crate) shown: Option<Rc<[usize]>>,
}

impl Lines {
    /// The cursor's place among the drawn rows: under a filter the scroll counts only those, and a row the filter
    /// hides has none.
    fn view(&self, cursor: usize) -> Option<usize> {
        match &self.shown {
            Some(rows) => rows.iter().position(|&row| row == cursor),
            None => Some(cursor),
        }
    }

    /// The folder being left: its cursor row's name and its line on screen.
    pub fn remember(&mut self, pane: &Pane) {
        if pane.busy() || pane.state.search || pane.path().is_empty() || pane.row_height() <= 0. {
            return;
        }
        let Some(row) = pane.held.row(pane.cursor) else {
            return;
        };
        let Some(view) = self.view(pane.cursor) else {
            return;
        };
        let line = view as f32 - pane.scroll_y / pane.row_height();
        if self.seen.len() >= KEPT && !self.seen.contains_key(pane.path()) {
            self.seen.clear();
        }
        let offset = self.grid.as_ref().map(|grid| grid.offset(pane.cursor));
        self.seen
            .insert(pane.path().to_owned(), (row.name.clone(), line, offset));
    }

    /// The cursor landed on `name`: put it back on the line it was left on, if this folder was left from it. Under
    /// Grid it answers the tile row's offset for Grid's own landing to apply.
    pub fn restore(&self, pane: &mut Pane, name: &str) -> Option<f32> {
        let (line, offset) = self
            .seen
            .get(pane.path())
            .filter(|(kept, ..)| kept == name)
            .map(|(_, line, offset)| (*line, *offset))?;
        if self.grid.is_some() {
            if let Some(offset) = offset {
                return Some(offset);
            }
        }
        let view = self.view(pane.cursor)?;
        let visible = pane.viewport_height() / pane.row_height();
        let line = line.clamp(0., (visible - 1.).max(0.));
        pane.scroll_to((view as f32 - line) * pane.row_height());
        pane.requests.initial_first = Some((pane.scroll_y / pane.row_height()).floor() as usize);
        None
    }
}

impl super::Navigator {
    /// keepScreenLine's landing on `name`: the List line now, or Grid's tile offset on its next frame.
    pub(super) fn restore_line(&mut self, pane: &mut Pane, name: &str) {
        if !self.keep_screen_line {
            return;
        }
        if let Some(offset) = self.lines.restore(pane, name) {
            self.grid_landing = Some((pane.held.listing, pane.cursor, offset));
        }
    }
}

impl crate::state::Shell {
    /// Settings > View "Keep the row's place when coming back", per window, read before any landing.
    pub(crate) fn sync_keep_screen_line(&mut self) {
        let keep = self
            .launch
            .settings
            .data
            .get("keepScreenLine")
            .and_then(flea::jsondoc::Json::as_bool)
            == Some(true);
        for host in self.panes.iter_mut().flatten() {
            host.nav.keep_screen_line = keep;
        }
    }
}

/// The last path component a pending select names.
pub fn leaf(target: &str) -> &str {
    target
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(target)
}

#[cfg(test)]
#[path = "position_tests.rs"]
mod tests;
