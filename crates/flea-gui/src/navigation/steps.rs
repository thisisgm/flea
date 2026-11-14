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
        self.pending_entry(pane).unwrap_or_else(|| self.drawn(pane))
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
        self.asked(pane).is_some() || (pane.path() != "/" && (pane.named || pane.busy()))
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
        let path = super::history::join(pane.path(), &row.name);
        let dir = flea::ui_logic_picker_portal::directory(row.dir, row.mode, &row.icon);
        if dir {
            self.open(pane, &path)?;
            return Ok(None);
        }
        Ok(Some(path))
    }

    // An operation changed the listing: re-read in place, re-selecting the path it produced.
    pub fn refresh(&mut self, pane: &mut Pane, select: Option<String>) -> Result<bool, Refusal> {
        let path = pane.path().to_owned();
        if pane.busy() {
            if pane.place() != path {
                return Err(Refusal::Busy);
            }
            // The older list did not ask for this target; let its pair finish before issuing the selected list.
            self.refresh_owed = Some((path, select));
            return Ok(false);
        }
        if self
            .anchor
            .take()
            .is_some_and(|anchor| anchor.need_paths.is_some())
        {
            self.gate.paths_pending = false;
        }
        self.pending_select = select;
        self.select_locate = None;
        let options = Options {
            kept_query: pane.state.query.clone(),
            in_place: true,
            independent_viewport: self.viewport.is_some(),
            restore_name: self
                .pending_select
                .as_deref()
                .and_then(|target| std::path::Path::new(target).strip_prefix(&path).ok())
                .map(|name| name.to_string_lossy().into_owned()),
            ..Options::default()
        };
        self.open_without_history(pane, &path, options, None, false)
    }

    pub(crate) fn persist(&mut self, patch: String, failure: String) {
        self.writer.borrow_mut().write(patch.clone(), failure);
        self.emit(Command::Persist(patch));
    }

    // Reload.js begin: a listing out refuses the reload, and an idle pane re-reads in place.
    pub fn reload(&mut self, pane: &mut Pane, selected: &[usize]) -> Result<bool, Refusal> {
        if pane.busy() {
            return Err(Refusal::Busy);
        }
        let asked = self.reread(pane, selected, true, false);
        if asked {
            self.reload_from = Some((pane.path().to_owned(), pane.held.total));
            self.reload_changed = None;
        }
        Ok(asked)
    }

    // Reload.js landed: the backend's added plus removed count, else the net change, owes a notice when not zero.
    pub(crate) fn land_reload(&mut self, pane: &Pane) {
        let Some((path, from)) = self.reload_from.take() else {
            return;
        };
        let changed = self.reload_changed.take();
        if path == pane.path() {
            let changed = changed.unwrap_or_else(|| pane.held.total.abs_diff(from));
            self.reloaded = (changed > 0).then_some(changed);
        }
    }

    // Reload.js begin past the wait: the stalled listing lets go as Anchor.clearWaiting does, then the folder waited on is asked again.
    pub fn retry(&mut self, pane: &mut Pane, selected: &[usize]) -> Result<bool, Refusal> {
        let waited = pane.place().to_owned();
        let now = pane.now();
        pane.swap.end(now, "retried");
        pane.state.kind = Kind::Loading;
        // A place whose rows never landed retries its navigation, so its first rows are no change.
        if pane.listed.as_deref() != Some(waited.as_str()) {
            return self.open_without_history(pane, &waited, Options::default(), None, true);
        }
        self.reload(pane, selected)
    }

    // The re-read PaneWire.qml onTransferDone asks for, taken here as a watch notice due at once, so the anchored re-read keeps the cursor's file, the marks and the view, and whatever owns the rows holds the debt; a search keeps its rows.
    pub fn refresh_after_transfer(
        &mut self,
        pane: &mut Pane,
        selected: &[usize],
        now: u64,
    ) -> bool {
        if pane.state.search {
            return false;
        }
        self.watch.owe(now);
        self.fire_watch(pane, selected, now)
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
        let deleted = self
            .deletion_anchor
            .take()
            .filter(|anchor| anchor.path == pane.path());
        // Anchor.js afterDelete: the cursor stands on the first deleted slot before the re-read, so a kept viewport lands on that slot and not on the row the cursor had.
        if let Some(slot) = deleted.as_ref().map(|anchor| anchor.index) {
            pane.cursor = slot.min(pane.held.total.saturating_sub(1));
        }
        let mut anchor = deleted.unwrap_or_else(|| {
            let mut anchor = Anchor::capture(pane, selected, select, want_changed);
            anchor.grid_offset = self.viewport.as_ref().map(|view| view.offset(pane.cursor));
            anchor
        });
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
            in_place: !want,
            independent_viewport: self.viewport.is_some(),
            restore_name: (!want && !anchor.name.is_empty()).then(|| anchor.name.clone()),
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
        self.locate_retry(pane);
    }

    pub(crate) fn landing_select(&mut self, pane: &mut Pane, selected: &mut Vec<usize>) {
        // A search arms its return while the query stands, so its own landings leave it armed.
        if pane.state.search {
            return;
        }
        if let Some(target) = self.pending_select.clone() {
            if anchor::apply_select(pane, &target, selected) {
                if self.keep_screen_line {
                    self.lines.restore(pane, super::position::leaf(&target));
                }
                self.pending_select = None;
                self.select_locate = None;
                self.landed = Some((pane.held.listing, pane.cursor));
            } else if pane.held.total == 0 {
                self.pending_select = None;
                self.select_locate = None;
            } else {
                self.locate_select(pane, &target);
            }
        }
    }
}

impl Navigator {
    /// Lists the history's paths over "/", the same request a folder takes; the reply keeps the order given.
    pub(crate) fn list_paths(
        &mut self,
        pane: &mut Pane,
        paths: &[String],
        hold_walk: bool,
    ) -> Result<bool, Refusal> {
        let preserve = std::mem::replace(&mut self.preserve_sort, true);
        let result = self.open_without_history(
            pane,
            "/",
            Options {
                supersede: true,
                hold_walk,
                ..Options::default()
            },
            None,
            false,
        );
        self.preserve_sort = preserve;
        if result == Ok(true) {
            self.orders.paths = true;
            if let Some(Command::List(listing)) = self.out.pop() {
                self.out.push(Command::ListPaths(listing, paths.to_vec()));
            }
        }
        result
    }
}
