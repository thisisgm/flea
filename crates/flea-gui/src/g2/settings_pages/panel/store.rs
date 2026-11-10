use super::{first_row, pages, Json, Panel, Store};

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

impl Panel {
    pub fn replace_store(&mut self, store: Box<dyn Store>) {
        let previous = pages::at(self.store.document(), &["places", "favourites"]);
        let next = pages::at(store.document(), &["places", "favourites"]);
        let changed = previous != next;
        let previous_count = previous
            .and_then(Json::as_array)
            .map_or(0, |rows| rows.len());
        let count = next.and_then(Json::as_array).map_or(0, |rows| rows.len());
        let cursor = self.cursor;
        self.store = store;
        self.rebuild();
        if self.section == "places" && changed {
            // SettingsPanel.qml onExternalChanged returns Favorite focus to Add, and shifts controls below it.
            if cursor > 0 && cursor <= previous_count.saturating_add(1) {
                self.cursor = first_row(&self.rows);
            } else if cursor > previous_count.saturating_add(1) {
                self.cursor = cursor
                    .saturating_add(count)
                    .saturating_sub(previous_count)
                    .min(self.rows.len().saturating_sub(1));
            }
            self.reveal();
        }
    }
}
