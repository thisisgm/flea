//! What a committed listing owes: the Back landing, the deep target, the anchor and the held sort.
use super::{history::join, Command, Navigator};
use crate::pane::{Kind, Pane};

impl Navigator {
    // Called after every reply the pane took; a new commit count means a listing landed.
    pub fn after_reply(&mut self, pane: &mut Pane, before: &str, selected: &mut Vec<usize>) {
        let commits = pane.swap.counters.commits;
        if commits != self.seen {
            self.seen = commits;
            if pane.path() != before {
                self.watch.clear();
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
            self.land_anchor(pane, selected);
            if let Some(at) = self.menus.landed(pane) {
                self.background_at = Some(at);
            }
            if let Some(preference) = self.preference.take() {
                if preference.path == pane.path() {
                    preference.apply(pane, selected);
                }
            }
            if self.relist_owed {
                let path = pane.path().to_owned();
                let _ = self.open(pane, &path);
            }
        } else {
            self.locate_parked(pane);
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
        pane.requests.initial_first =
            Some((pane.scroll_y / crate::pane::ROW_HEIGHT).floor() as usize);
    }

    fn land_anchor(&mut self, pane: &mut Pane, selected: &mut Vec<usize>) {
        let Some(mut anchor) = self.anchor.take() else {
            return;
        };
        if anchor.need_paths.is_some() {
            self.anchor = Some(anchor);
            return;
        }
        if let Some(paths) = anchor.apply(pane, selected) {
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
    pub fn take_located(&mut self, pane: &mut Pane, directory: &str, path: &str, index: i64) -> bool {
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
        self.anchor = None;
        true
    }

    // A listing that ended in a refusal owes no landing, and its anchor must not hold the watch forever.
    pub fn after_failure(&mut self, pane: &mut Pane, operation: &str) {
        if matches!(operation, "paths" | "stale") && self.gate.paths_pending {
            self.take_paths(pane, &[]);
            return;
        }
        let ended = !pane.busy() && matches!(pane.state.kind, Kind::Error | Kind::Locked);
        self.menus.failed(operation, ended);
        if ended {
            self.restore = None;
            // Only a failed Esc relist drops the cursor file; any other ended listing keeps it armed (ui/js/Swap.js failListing).
            if std::mem::take(&mut self.relisting) {
                self.pending_select = None;
            }
            self.relist_owed = false;
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
