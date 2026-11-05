//! What a committed listing owes: the Back landing, the deep target, the anchor and the held sort.
use super::{
    history::{join, select_match},
    Command, Navigator,
};
use crate::pane::{Kind, Pane};

impl Navigator {
    // Called after every reply the pane took; a new commit count means a listing landed.
    pub fn after_reply(&mut self, pane: &mut Pane, before: &str, selected: &mut Vec<usize>) {
        self.sync_orders(pane);
        let commits = pane.swap.counters.commits;
        if commits != self.seen {
            self.seen = commits;
            if let Some((path, select)) = self.refresh_owed.take() {
                // An owed watch refresh can supply fresh rows; a matching name alone cannot.
                let names = pane.held.rows.iter().map(|row| row.name.as_str());
                let held = select
                    .as_deref()
                    .is_some_and(|target| select_match(names, target, &path).is_some());
                if path == pane.path() && held && self.watch.stale {
                    self.pending_select = select;
                } else if path == pane.path() {
                    let _ = self.refresh(pane, select);
                    return;
                }
            }
            if pane.path() != before {
                self.watch.clear();
                self.deletion_anchor = None;
                self.grid_landing = None;
            }
            if self
                .anchor
                .as_ref()
                .is_some_and(|anchor| anchor.path != pane.path())
            {
                self.anchor = None;
            }
            self.land_restore(pane);
            self.landing_select(pane, selected);
            self.land_reload(pane);
            self.land_anchor(pane, selected);
            if let Some(at) = self.menus.landed(pane) {
                self.background_at = Some(at);
            }
            if let Some(preference) = self.preference.take() {
                if preference.path == pane.path() {
                    preference.apply(pane, selected);
                }
            }
            // An own delete answered while this listing was out re-reads now, so its replacement row is selected.
            if self
                .deletion_anchor
                .as_ref()
                .is_some_and(|anchor| anchor.path == pane.path())
            {
                let _ = self.reread(pane, &[], false, true);
            }
        } else {
            if let Some(cursor) = pane
                .swap
                .pending
                .as_ref()
                .and_then(|pending| pending.retained_cursor)
            {
                if let Some(anchor) = self.anchor.as_mut() {
                    anchor.index = cursor;
                }
            }
            if let Some(start) = pane
                .swap
                .pending
                .as_ref()
                .and_then(|pending| pending.retained_start)
            {
                if let Some(request) = pane.anchor_window(start) {
                    self.emit(Command::Window {
                        start: request.start,
                        count: request.count,
                    });
                }
            }
            self.locate_parked(pane);
            if !pane.busy() {
                self.landing_select(pane, selected);
            }
        }
        let now = pane.now();
        self.settle(pane, now);
    }

    // Name, then clamped index, then the top; a walk or an empty listing keeps the commit's own cursor.
    fn land_restore(&mut self, pane: &mut Pane) {
        let Some(restore) = self.restore.take() else {
            return;
        };
        let total = pane.held.total;
        if restore.entry.path != pane.path() || pane.state.search || total == 0 {
            return;
        }
        let name = restore.entry.name.as_str();
        pane.cursor = super::anchor::index_of(pane, name)
            .unwrap_or_else(|| restore.entry.index.min(total - 1));
        pane.reveal_cursor();
        if self.keep_screen_line {
            self.lines.restore(pane, name);
        }
        pane.requests.initial_first = Some((pane.scroll_y / pane.row_height()).floor() as usize);
    }

    fn land_anchor(&mut self, pane: &mut Pane, selected: &mut Vec<usize>) {
        let Some(mut anchor) = self.anchor.take() else {
            return;
        };
        if anchor.need_paths.is_some() {
            self.anchor = Some(anchor);
            return;
        }
        let paths = anchor.apply(pane, selected);
        if paths.is_none() && anchor.had_marks && anchor.lone {
            self.lone_mark = selected.first().copied();
        }
        if anchor.landed {
            self.grid_landing = anchor
                .grid_offset
                .map(|offset| (pane.held.listing, pane.cursor, offset));
        }
        if let Some(paths) = paths {
            let id = self.next_locate();
            anchor.locate = Some(id);
            self.anchor = Some(anchor);
            self.emit(Command::LocateMany { paths, id });
        }
    }

    // A Back whose first window lacks the owed name asks where it is; the rows wait for its window.
    fn locate_parked(&mut self, pane: &Pane) {
        let Some(pending) = pane.swap.pending.as_ref() else {
            return;
        };
        let (Some(ready), Some(_)) = (pending.ready.as_ref(), pending.parked_until) else {
            return;
        };
        let Some(restore) = self.restore.as_mut() else {
            return;
        };
        if restore.locate.is_some() || restore.entry.path != pending.path {
            return;
        }
        let path = join(&restore.entry.path, &restore.entry.name);
        restore.locate = Some(path.clone());
        restore.total = ready.total;
        self.emit(Command::Locate { path });
    }

    // The locate reply for a waiting landing fetches the window holding the row, or the clamped index on a miss.
    pub fn take_located(
        &mut self,
        pane: &mut Pane,
        directory: &str,
        path: &str,
        index: i64,
    ) -> bool {
        let Some(restore) = self.restore.as_mut() else {
            return false;
        };
        if restore.fetched
            || restore.locate.as_deref() != Some(path)
            || restore.entry.path != directory
            || restore.total == 0
        {
            return false;
        }
        let total = restore.total;
        let index = usize::try_from(index)
            .ok()
            .filter(|index| *index < total)
            .unwrap_or_else(|| restore.entry.index.min(total - 1));
        restore.fetched = true;
        if let Some(request) = pane.anchor_window(index) {
            self.emit(Command::Window {
                start: request.start,
                count: request.count,
            });
        }
        true
    }

    // Only the anchor's own batch reply resolves it; a refused one lands what is already kept.
    pub fn take_located_many(
        &mut self,
        pane: &Pane,
        directory: &str,
        id: u64,
        ok: bool,
        matches: &[(String, usize)],
        selected: &mut Vec<usize>,
    ) -> bool {
        let Some(anchor) = self.anchor.as_mut() else {
            return false;
        };
        if anchor.locate != Some(id) || anchor.locate_done || directory != pane.path() {
            return false;
        }
        anchor.located(if ok { matches } else { &[] }, selected);
        if anchor.lone {
            self.lone_mark = selected.first().copied();
        }
        self.anchor = None;
        true
    }

    // A listing that ended in a refusal owes no landing, and its anchor must not hold the watch forever.
    pub fn after_failure(&mut self, pane: &mut Pane, operation: &str) {
        if matches!(operation, "locate" | "window" | "backend" | "stale")
            && self.select_locate.is_some()
        {
            self.pending_select = None;
            self.select_locate = None;
        }
        if operation == "sort" {
            if let Some(at) = self.orders.requests.iter().position(|request| request.sort) {
                self.orders.requests.remove(at);
            }
            self.order = self
                .orders
                .requests
                .back()
                .map(|request| request.order)
                .unwrap_or(self.drawn_order());
        }
        if matches!(operation, "paths" | "stale") && self.gate.paths_pending {
            self.take_paths(pane, &[]);
            return;
        }
        let ended = !pane.busy() && matches!(pane.state.kind, Kind::Error | Kind::Locked);
        self.menus.failed(operation, ended);
        if ended {
            self.refresh_owed = None;
            self.select_locate = None;
            self.orders.requests.clear();
            self.order = self.drawn_order();
            self.orders.path = pane.path().into();
            self.restore = None;
            // A failed reload owes no notice, so a later landing of the same folder must not spend it.
            self.reload_from = None;
            self.reload_changed = None;
            // Only a failed Esc relist drops the cursor file; any other ended listing keeps it armed (ui/js/Swap.js failListing).
            if std::mem::take(&mut self.relisting) {
                self.pending_select = None;
            }
            self.deletion_anchor = None;
            if self
                .anchor
                .as_ref()
                .is_some_and(|anchor| anchor.need_paths.is_none())
            {
                self.anchor = None;
            }
        }
        // The names on disk changed in a way no reply listed, so re-read the listing and select nothing.
        if matches!(operation, "rename-kept" | "rename-stranded") {
            let _ = self.refresh(pane, None);
        }
    }
}

#[cfg(test)]
#[path = "landing_window_tests.rs"]
mod window_tests;
