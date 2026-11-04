//! Where the cursor stood on screen in each folder a pane left, so coming back draws the list as it was.
//!
//! A landing on a remembered folder's remembered row scrolls so that row sits on the same screen line,
//! rather than at the edge a plain reveal leaves it. Each pane's navigator keeps its own, and only while
//! ui.json "keepScreenLine" is on for its window.
use crate::pane::Pane;
use std::collections::HashMap;

const KEPT: usize = 512;

#[derive(Default)]
pub struct Lines {
    /// Folder path to the cursor row's name and its line on screen, in rows from the top.
    seen: HashMap<String, (String, f32)>,
}

impl Lines {
    /// The folder being left: its cursor row's name and its line on screen.
    pub fn remember(&mut self, pane: &Pane) {
        if pane.busy() || pane.state.search || pane.path().is_empty() || pane.row_height() <= 0. {
            return;
        }
        let Some(row) = pane.held.row(pane.cursor) else {
            return;
        };
        let line = pane.cursor as f32 - pane.scroll_y / pane.row_height();
        if self.seen.len() >= KEPT && !self.seen.contains_key(pane.path()) {
            self.seen.clear();
        }
        self.seen
            .insert(pane.path().to_owned(), (row.name.clone(), line));
    }

    /// The cursor landed on `name`: put it back on the line it was left on, if this folder was left from it.
    pub fn restore(&self, pane: &mut Pane, name: &str) {
        let Some(line) = self
            .seen
            .get(pane.path())
            .filter(|(kept, _)| kept == name)
            .map(|(_, line)| *line)
        else {
            return;
        };
        let visible = pane.viewport_height() / pane.row_height();
        let line = line.clamp(0., (visible - 1.).max(0.));
        pane.scroll_to((pane.cursor as f32 - line) * pane.row_height());
        pane.requests.initial_first = Some((pane.scroll_y / pane.row_height()).floor() as usize);
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
