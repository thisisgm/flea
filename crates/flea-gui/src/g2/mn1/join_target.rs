//! A Columns right click's unselected target, adopted by a row action that reads the listing's cursor.
use super::*;

impl Shell {
    /// A row action that reads the listing's own cursor first takes the target a Columns right click left unselected.
    pub(crate) fn adopt_menu_target(&mut self, captured: &Captured) {
        let Some(host) = self.panes[captured.side].as_mut() else {
            return;
        };
        if captured.background || host.pane.held.listing != captured.listing {
            return;
        }
        host.pane.cursor = captured.cursor;
        if captured.rows.len() > 1 {
            host.selected = captured.rows.clone();
        } else if host.selected != captured.rows {
            host.selected.clear();
        }
    }
}
