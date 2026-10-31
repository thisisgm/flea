//! ListView's setters for what row text reads: each change bumps the revision the shaped-row cache keys on.
use super::ListView;

impl ListView {
    pub fn set_searching(&mut self, searching: bool) {
        if self.searching != searching {
            self.searching = searching;
            self.text += 1;
        }
    }

    pub fn set_recent(&mut self, recent: bool) {
        if self.recent != recent {
            self.recent = recent;
            self.text += 1;
        }
    }

    pub fn set_query(&mut self, query: &str) {
        if self.query != query {
            self.query.clear();
            self.query.push_str(query);
            self.text += 1;
        }
    }

    pub fn text_revision(&self) -> u64 {
        self.text
    }
}
