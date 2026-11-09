//! The unseen warm table: once, on the first rested subject that holds a table, the table's faces are shaped before
//! Quick Look opens, so its first frame never waits on them. Never under a held key, never once Quick Look is open.

/// The warm's state and its counts, as tests/quicklook-tablewarm.qml reads them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableWarm {
    /// Warm tables built, at most one.
    pub warms: usize,
    pub warmed: bool,
    /// The subject a warm was asked for and has not answered.
    pub asked: Option<String>,
}

impl TableWarm {
    /// The cursor rested on `path`; true when a warm goes out for it now.
    pub fn rested(&mut self, path: &str, holds_table: bool, looking: bool) -> bool {
        if self.warmed || self.asked.is_some() || !holds_table || looking {
            return false;
        }
        self.asked = Some(path.to_string());
        true
    }

    /// The warm for `path` answered with the cursor on `current`: builds only if the cursor stayed and Space is up.
    pub fn answered(&mut self, path: &str, current: &str, looking: bool) -> bool {
        if self.asked.as_deref() != Some(path) {
            return false;
        }
        self.asked = None;
        if looking || path != current {
            return false;
        }
        self.warmed = true;
        self.warms += 1;
        true
    }
}

#[cfg(test)]
#[path = "table_warm_tests.rs"]
mod tests;
