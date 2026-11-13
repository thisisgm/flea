//! One burst of foreign writes is one re-read, held while an interaction owns the rows.

// The deadline absorbs later notifications instead of restarting, so continuous change still settles.
pub const WATCH_MS: u64 = 400;

// The states that own the rows; a re-read renumbers every row, so it waits while any is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gate {
    pub pointer_held: bool,
    pub dragging: bool,
    pub menu_open: bool,
    pub menu_pending: bool,
    pub editing: bool,
    pub rename_pending: bool,
    pub filter_typing: bool,
    pub search: bool,
    pub selection_band: bool,
    pub collision_pending: bool,
    pub paths_pending: bool,
    pub clip_pending: bool,
}

impl Gate {
    pub fn owned(&self) -> bool {
        self.pointer_held
            || self.dragging
            || self.menu_open
            || self.menu_pending
            || self.editing
            || self.rename_pending
            || self.filter_typing
            || self.search
            || self.selection_band
            || self.collision_pending
            || self.paths_pending
            || self.clip_pending
    }
}

#[derive(Default, Debug)]
pub struct Watch {
    pub stale: bool,
    pub due: Option<u64>,
    pub notices: usize,
    pub rereads: usize,
}

impl Watch {
    // Unsolicited; a notice for a directory the pane has left says nothing about this one.
    pub fn changed(&mut self, path: &str, drawn: &str, now: u64) -> bool {
        if path != drawn {
            return false;
        }
        self.notices += 1;
        self.stale = true;
        if self.due.is_none() {
            self.due = Some(now.saturating_add(WATCH_MS));
        }
        true
    }

    // The pane's own finished write: the re-read is owed now, and it pays any notice still waiting with it.
    pub fn owe(&mut self, now: u64) {
        self.stale = true;
        self.due = Some(now);
    }

    // True when the owed re-read is due and nothing holds the rows; a held debt is kept, not dropped.
    pub fn fire(&mut self, now: u64, busy: bool) -> bool {
        match self.due {
            Some(due) if now >= due => {
                self.due = None;
                if !self.stale || busy {
                    return false;
                }
                self.stale = false;
                self.rereads += 1;
                true
            }
            _ => false,
        }
    }

    // Busy going false pays a held debt through the same deadline.
    pub fn settle(&mut self, now: u64, busy: bool) {
        if self.stale && !busy && self.due.is_none() {
            self.due = Some(now.saturating_add(WATCH_MS));
        }
    }

    pub fn wait(&self, now: u64) -> Option<u64> {
        self.due.map(|due| due.saturating_sub(now))
    }

    // A debt owed by the directory the pane left is not owed by the one it arrived in.
    pub fn clear(&mut self) {
        self.stale = false;
        self.due = None;
    }
}

// inotify hears nothing a FUSE mount or a network share changes on its far side, so only those folders re-read when the window returns.
pub fn blind(fs: &str, class: &str) -> bool {
    fs == "fuse" || matches!(class, "network" | "phone")
}

// The window's activation as a latch: only a return after leaving owes a re-read, never the first activation or a repeat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Front {
    #[default]
    Unseen,
    Shown,
    Left,
}

impl Front {
    pub fn turn(&mut self, active: bool) -> bool {
        let returned = active && *self == Front::Left;
        *self = match (active, *self) {
            (true, _) => Front::Shown,
            (false, Front::Unseen) => Front::Unseen,
            (false, _) => Front::Left,
        };
        returned
    }
}

#[cfg(test)]
#[path = "watch_tests.rs"]
mod tests;
