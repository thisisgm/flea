//! The filter strip as the window hosts it: slot height, caret rule and select-all guard (sr2 owns the match).
use crate::list::Metrics;
use gpui::Pixels;
use std::collections::HashSet;

/// The strip's height for Slots::strip_height: Theme.chromeHeight, the list header metric at the live body size.
pub fn height(body: Pixels) -> f32 {
    Metrics::at(body).header
}

/// The caret stands only while the query line holds the keyboard: Enter hands it back to the list.
pub fn caret(typing: bool, line_focused: bool) -> bool {
    typing && line_focused
}

/// What Ctrl+A probes per exact row: one membership verdict per call, so the work stays linear in the exact rows.
pub trait CloseRows {
    fn has(&self, row: usize) -> bool;
}

/// The drawn close rows as one lookup built once; the join hands the drawn answer over at the join.
pub struct CloseSet {
    set: HashSet<usize>,
}

impl CloseSet {
    /// One lookup built once from the drawn close rows; an empty answer builds nothing to probe.
    pub fn new(close: &[usize]) -> Self {
        Self {
            set: close.iter().copied().collect(),
        }
    }
}

impl CloseRows for CloseSet {
    fn has(&self, row: usize) -> bool {
        self.set.contains(&row)
    }
}

/// Ctrl+A marks the exact rows, never a close one: an empty exact list returns without probing, else one lookup per exact row.
pub fn select_exact(exact: &[usize], close: &impl CloseRows) -> Vec<usize> {
    if exact.is_empty() {
        return Vec::new();
    }
    exact
        .iter()
        .copied()
        .filter(|row| !close.has(*row))
        .collect()
}
