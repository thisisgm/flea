//! The Recent place in a pane: the history listed under "/", newest first, and every way back out.
use std::collections::HashMap;

pub type Visits = HashMap<String, Option<i64>>;

// What a pane does for Recent; the pane owns its listing, sort and navigation, Recent owns when to ask.
pub trait RecentHost {
    fn list_in_flight(&self) -> bool;
    // A history takes the place of a listing out: stop waiting on it and drop what it owed (Swap.stopWaiting, BackReturn.dropOwed).
    fn supersede_listing(&mut self);
    fn sort(&self) -> (String, bool);
    fn set_sort(&mut self, by: &str, desc: bool);
    // Where the pane stands now, the way back (Nav.place).
    fn place(&self) -> String;
    // A history replaces the standing listing, so a live search session closes first; true when a walk was leaving.
    fn leave_walk(&mut self) -> bool;
    // Stand on "/" with no fs figures, hold or forget the old rows, and ask listpaths for these paths.
    fn list_paths(&mut self, paths: &[String], hold_walk: bool);
    fn message(&mut self, key: &'static str);
    fn set_pending_select(&mut self, path: &str);
    fn open_without_history(&mut self, path: &str, supersede: bool);
    // Ask the rail to re-read the history for this pane; false when no rail is loaded.
    fn reread_history(&mut self) -> bool;
    // The full path of the row under the cursor, None while it has not loaded.
    fn cursor_path(&self) -> Option<String>;
}

pub const BUSY_KEY: &str = "pane.busy";
pub const PASTE_REFUSED_KEY: &str = "places.recent.paste_refused";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecentMode {
    pub on: bool,
    pub from: String,
    pub paths: Vec<String>,
    pub visits: Visits,
    // The order run() replaced, handed back on every way out.
    pub stashed: Option<(String, bool)>,
}

impl RecentMode {
    // One refusal for both paste routes; true when it refused.
    pub fn refuse_paste(&self, host: &mut impl RecentHost) -> bool {
        if self.on {
            host.message(PASTE_REFUSED_KEY);
        }
        self.on
    }

    // Open keeps where the pane stood and asks the history's paths; a second open takes the first one's place.
    pub fn run(&mut self, host: &mut impl RecentHost, paths: Vec<String>, visits: Visits) {
        if host.list_in_flight() {
            host.supersede_listing();
        }
        if !self.on {
            self.stashed = Some(host.sort());
        }
        if self.from.is_empty() {
            self.from = host.place();
        }
        let hold_walk = host.leave_walk();
        self.paths = paths;
        self.visits = visits;
        self.on = true;
        host.list_paths(&self.paths, hold_walk);
        host.set_sort("mtime", true);
    }

    // The order run() replaced, handed back before the folder re-lists.
    pub fn restore_sort(&mut self, host: &mut impl RecentHost) {
        if let Some((by, desc)) = self.stashed.take().filter(|(by, _)| !by.is_empty()) {
            host.set_sort(&by, desc);
        }
    }

    // Every way out, a hop, Escape, a tab switch and reveal, leaves through this one step.
    pub fn leave(&mut self, host: &mut impl RecentHost) {
        if self.on {
            self.restore_sort(host);
        }
        self.on = false;
        self.from.clear();
        self.paths.clear();
        self.visits.clear();
    }

    // Leaving Recent re-lists the folder it was opened over, as the newest request and with no history entry.
    pub fn close(&mut self, host: &mut impl RecentHost) {
        let back = if self.from.is_empty() {
            "/".to_owned()
        } else {
            self.from.clone()
        };
        self.leave(host);
        host.open_without_history(&back, true);
    }

    // Refresh re-reads history after an operation and never re-lists "/" over it; with no rail it re-asks the standing paths.
    pub fn refresh(&mut self, host: &mut impl RecentHost, select: &str) {
        if host.list_in_flight() {
            host.message(BUSY_KEY);
            return;
        }
        host.set_pending_select(select);
        if host.reread_history() {
            return;
        }
        let (paths, visits) = (self.paths.clone(), self.visits.clone());
        self.run(host, paths, visits);
    }

    // o opens the cursor row's own folder with the cursor on it; Enter opens the file, so the two cannot disagree.
    pub fn reveal(&mut self, host: &mut impl RecentHost) {
        if host.list_in_flight() {
            host.message(BUSY_KEY);
            return;
        }
        let Some(full) = host.cursor_path() else {
            return;
        };
        let Some(cut) = full.rfind('/') else { return };
        let dir = if cut == 0 {
            "/".to_owned()
        } else {
            full[..cut].to_owned()
        };
        self.leave(host);
        host.set_pending_select(&full);
        host.open_without_history(&dir, false);
    }

    // The folder a tab records on a history, None elsewhere, so it lands where it stood.
    pub fn resting_path(&self) -> Option<&str> {
        (self.on && !self.from.is_empty()).then_some(self.from.as_str())
    }

    // A tab switch drops the mode and says whether it dropped one.
    pub fn drop_overlay(&mut self, host: &mut impl RecentHost) -> bool {
        let was = self.on;
        if was {
            self.leave(host);
        }
        was
    }

    // The Used stamp for one held row, by its full path; None off Recent or for a path the history never visited.
    pub fn used(&self, full_path: &str) -> Option<i64> {
        if !self.on {
            return None;
        }
        self.visits.get(full_path).copied().flatten()
    }
}
