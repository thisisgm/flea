// Menu source identities remain pinned from validation through the operation's worker.
use super::{MenuActions, Selected, Snapshot, MENU_STOPPED, SELECTION_CHANGED};

impl MenuActions {
    pub(crate) fn selection(&self, id: usize) -> Result<Vec<Selected>, String> {
        let snapshot = self.snapshot.lock().map_err(|_| MENU_STOPPED)?;
        if id == 0 || snapshot.id != id || snapshot.items.is_empty() {
            return Err(SELECTION_CHANGED.into());
        }
        Ok(snapshot.items.clone())
    }

    pub(crate) fn extract_selection(&self, id: usize) -> Result<Vec<Selected>, String> {
        let snapshot = self.snapshot.lock().map_err(|_| MENU_STOPPED)?;
        if id == 0 || snapshot.id != id {
            return Err(SELECTION_CHANGED.into());
        }
        Ok(vec![snapshot.extract_item()?.clone()])
    }
}

impl Snapshot {
    pub(super) fn extract_item(&self) -> Result<&Selected, String> {
        let [item] = self.items.as_slice() else {
            return Err(SELECTION_CHANGED.into());
        };
        // Older callers capture one item without a separate cursor identity.
        Ok(self.cursor.as_ref().unwrap_or(item))
    }
}
