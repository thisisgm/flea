//! #506: a plain left press on a row outside the selection selects that row on press, as Finder and Nautilus do.
//! A press on a row inside the selection still decides on release, so the whole selection can be lifted and dragged.
use crate::{
    state::Shell,
    window::{input::pointer, startup::View},
};
use gpui::{MouseButton, MouseDownEvent};

impl Shell {
    /// Selects the pressed row now when its release would only select it, and keeps the slow-click state the
    /// release needs: whether the row was the sole selection before this press.
    pub(super) fn select_on_press(
        &mut self,
        side: usize,
        outline: Option<usize>,
        event: &MouseDownEvent,
    ) {
        // Columns move their cursor on press already, and a directory press there opens a column on release.
        if event.button != MouseButton::Left
            || event.click_count != 1
            || self.view != View::List && self.view != View::Grid
            || self.shown_view() == View::Columns
        {
            return;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(row) = host.input.press.as_ref().map(|press| press.row) else {
            return;
        };
        if outline == Some(row) || host.selected.binary_search(&row).is_ok() {
            return;
        }
        let kind = if host.search.owner.session.reveals_on_activate() {
            "result"
        } else if host.pane.held.row(row).is_some_and(|facts| facts.dir) {
            "dir"
        } else {
            "file"
        };
        // The release would select only this row: the binding says so, not a hard-coded button.
        let effect = pointer("listing", kind, event).map(|binding| binding.effect);
        if !matches!(effect, Some("selectOnly" | "open")) {
            return;
        }
        // The row was outside the selection, so it was not the sole selection: no slow-click rename on this release.
        if let Some(press) = host.input.press.as_mut() {
            press.was_sole = Some(false);
        }
        host.input.selection.only(&mut host.selected, row);
        host.nav.landed = None;
        // No scroll here: a reveal could move the row from under the pointer, and the release reveals it anyway.
        host.pane.cursor = row;
    }
}

#[cfg(test)]
#[path = "press_select_tests.rs"]
mod tests;
