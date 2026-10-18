//! Each navigation step as one decision over the pane's committed snapshot.
use super::{
    anchor::{self, cursor_name, Anchor},
    history::{parent_of, trim_slash, Entry, Restore, Stack},
    Command, Navigator, Refusal,
};
use crate::pane::{swap::Options, Kind, Pane};

fn supersede() -> Options {
    Options {
        supersede: true,
        ..Options::default()
    }
}

impl Navigator {
    // The place a listing still out asked for, with the cursor its Back or Forward owes it.
    pub fn pending_entry(&self, pane: &Pane) -> Option<Entry> {
        if !pane.busy() || pane.place() == pane.path() {
            return None;
        }
        let place = pane.place();
        Some(match &self.restore {
            Some(owed) if owed.entry.path == place => owed.entry.clone(),
            _ => Entry::new(place, "", 0),
        })
    }

    // The drawn folder with its cursor row; leaving a walk names no row, so it lands at the top.
    pub fn drawn(&self, pane: &Pane) -> Entry {
        if pane.state.search {
            return Entry::new(pane.path(), "", 0);
        }
        Entry::new(pane.path(), cursor_name(pane), pane.cursor)
    }

    pub fn capture(&self, pane: &Pane) -> Entry {
        self.pending_entry(pane)
            .unwrap_or_else(|| self.drawn(pane))
    }

    // A new destination discards the forward branch and pushes the drawn place unless the listing out already did.
    pub fn open(&mut self, pane: &mut Pane, path: &str) -> Result<bool, Refusal> {
        let place = pane.place().to_owned();
        if !place.is_empty() && path != place {
            match self.pending_entry(pane) {
                None => {
                    let entry = self.drawn(pane);
                    self.history.push(Stack::Back, entry);
                }
                Some(_)
                    if self
                        .restore
                        .as_ref()
                        .is_some_and(|owed| owed.pushed_onto == Stack::Forward) =>
                {
                    let entry = self.drawn(pane);
                    self.history.push(Stack::Back, entry);
                }
                Some(_) => {}
            }
            self.history.forward.clear();
        }
        self.open_without_history(pane, path, supersede(), None, false)
    }

    fn travel(&mut self, pane: &mut Pane, from: Stack, to: Stack) -> Result<bool, Refusal> {
        let Some(target) = self.history.stack(from).last().cloned() else {
            return Ok(false);
        };
        let entry = self.capture(pane);
        self.history.push(to, entry);
        self.history.stack(from).pop();
        let path = target.path.clone();
        let restore = Restore {
            entry: target,
            pushed_onto: to,
            locate: None,
            total: 0,
            fetched: false,
        };
        self.open_without_history(pane, &path, supersede(), Some(restore), false)
    }

    pub fn back(&mut self, pane: &mut Pane) -> Result<bool, Refusal> {
        self.travel(pane, Stack::Back, Stack::Forward)
    }

    pub fn forward(&mut self, pane: &mut Pane) -> Result<bool, Refusal> {
        self.travel(pane, Stack::Forward, Stack::Back)
    }

    // The mouse back button follows history, or climbs when no history exists.
    pub fn mouse_back(&mut self, pane: &mut Pane) -> Result<bool, Refusal> {
        if self.history.back.is_empty() {
            self.parent(pane)
        } else {
            self.back(pane)
        }
    }

    // A hop that is out, refused or waiting climbs from the folder it asked for.
    pub fn asked(&self, pane: &Pane) -> Option<String> {
        let asked = if pane.busy() {
            pane.place()
        } else if matches!(pane.state.kind, Kind::Locked | Kind::Waiting) {
            pane.state.target.as_str()
        } else {
            return None;
        };
        (!asked.is_empty() && asked != pane.path()).then(|| asked.to_owned())
    }

    // Backspace and the chrome's up arrow stop at the root; the parent lands on the directory left.
    pub fn parent(&mut self, pane: &mut Pane) -> Result<bool, Refusal> {
        if let Some(asked) = self.asked(pane) {
            self.pending_select = Some(trim_slash(&asked).to_owned());
            let up = parent_of(&asked).to_owned();
            return self.open(pane, &up);
        }
        if pane.path() == "/" {
            return Ok(false);
        }
        let here = pane.path().to_owned();
        self.pending_select = Some(here.clone());
        self.open(pane, parent_of(&here))
    }

    pub fn can_go_up(&self, pane: &Pane) -> bool {
        self.asked(pane).is_some() || pane.path() != "/"
    }

    // A click on the folder already shown is a no-op.
    pub fn showing(&self, pane: &Pane, path: &str) -> bool {
        trim_slash(path) == pane.path()
            && !pane.busy()
            && !pane.state.search
            && matches!(pane.state.kind, Kind::Ready | Kind::Empty)
    }

    // Same-path Enter re-lists only a failed or locked listing.
    pub fn path_failed(&self, pane: &Pane) -> bool {
        !pane.busy() && !pane.state.search && matches!(pane.state.kind, Kind::Error | Kind::Locked)
    }

    pub fn open_place(&mut self, pane: &mut Pane, path: &str) -> Result<bool, Refusal> {
        if self.showing(pane, path) {
            return Ok(false);
        }
        self.open(pane, path)
    }

    // Enter on the cursor row: a directory navigates; anything else is the opener's.
    pub fn open_cursor(&mut self, pane: &mut Pane) -> Result<Option<String>, Refusal> {
        if pane.busy() {
            return Err(Refusal::Busy);
        }
        let row = pane.held.row(pane.cursor).ok_or(Refusal::NotLoaded)?;
        let (path, dir) = (super::history::join(pane.path(), &row.name), row.dir);
        if dir {
            self.open(pane, &path)?;
            return Ok(None);
        }
        Ok(Some(path))
    }

    // An operation changed the listing: re-read in place, re-selecting the path it produced.
    pub fn refresh(&mut self, pane: &mut Pane, select: Option<String>) -> Result<bool, Refusal> {
        self.pending_select = select;
        let path = pane.path().to_owned();
        let options = Options {
            kept_query: pane.state.query.clone(),
            ..Options::default()
        };
        self.open_without_history(pane, &path, options, None, false)
    }

    // Hidden is a scan filter, so changing it is a fresh list back at row 0, never a sort.
    pub fn toggle_hidden(&mut self, pane: &mut Pane, failure: String) -> Result<bool, Refusal> {
        let patch = self.folders.borrow_mut().toggle_hidden();
        self.persist(patch, failure);
        let path = pane.path().to_owned();
        self.open(pane, &path)
    }

    pub(crate) fn persist(&mut self, patch: String, failure: String) {
        self.writer.borrow_mut().write(patch.clone(), failure);
        self.emit(Command::Persist(patch));
    }

    // The owed re-read: the anchor rides the list so its first rows hold the cursor's file.
    pub fn reread(
        &mut self,
        pane: &mut Pane,
        selected: &[usize],
        want_changed: bool,
        select: bool,
    ) -> bool {
        if pane.busy() {
            return false;
        }
        let mut anchor = Anchor::capture(pane, selected, select, want_changed);
        let need = anchor.unnamed();
        if !need.is_empty() && !self.gate.paths_pending {
            if let Some(action) = pane.row_action(need.clone()) {
                anchor.need_paths = Some(need);
                self.gate.paths_pending = true;
                self.anchor = Some(anchor);
                self.emit(Command::Paths(action));
                return true;
            }
        }
        self.start_anchor(pane, anchor)
    }

    fn start_anchor(&mut self, pane: &mut Pane, anchor: Anchor) -> bool {
        if anchor.path != pane.path() {
            self.anchor = None;
            return false;
        }
        let path = anchor.path.clone();
        let want = anchor.want_changed;
        let options = Options {
            kept_query: pane.state.query.clone(),
            ..Options::default()
        };
        self.anchor = Some(anchor);
        let started = self
            .open_without_history(pane, &path, options, None, want)
            .unwrap_or(false);
        if !started {
            self.anchor = None;
        }
        started
    }

    // The batched paths answer for the anchor fills unheld names, then starts the held list.
    pub fn take_paths(&mut self, pane: &mut Pane, paths: &[String]) -> bool {
        let Some(mut anchor) = self.anchor.take().filter(|a| a.need_paths.is_some()) else {
            return false;
        };
        self.gate.paths_pending = false;
        anchor.fill_paths(paths);
        self.start_anchor(pane, anchor);
        true
    }

    pub fn changed(&mut self, pane: &Pane, path: &str, now: u64) -> bool {
        self.watch.changed(path, pane.path(), now)
    }

    pub fn fire_watch(&mut self, pane: &mut Pane, selected: &[usize], now: u64) -> bool {
        let busy = self.busy(pane);
        if !self.watch.fire(now, busy) {
            self.watch.settle(now, busy);
            return false;
        }
        self.reread(pane, selected, false, false)
    }

    pub fn settle(&mut self, pane: &Pane, now: u64) {
        let busy = self.busy(pane);
        self.watch.settle(now, busy);
    }

    pub(crate) fn landing_select(&mut self, pane: &mut Pane, selected: &mut Vec<usize>) {
        if let Some(target) = self.pending_select.take() {
            if !pane.state.search {
                anchor::apply_select(pane, &target, selected);
            }
        }
    }
}
