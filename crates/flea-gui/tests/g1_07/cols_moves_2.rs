//! The columnsmoves fixture: its directory map, what a frame draws, when a move has settled, and the reads its controls make.
use super::check::Checks;
use super::cols_moves::{answered, child_of, Ask, Dirs, Sim, FRAME_MS};
use super::fixture::{Pane, SETTINGS};
use flea_gui::columns::{
    geometry::Slot,
    peeks::{peek_key, Facts, Lens, PeekRow, Peeked},
    Area, TrackRows, TrackView,
};
use flea_gui::navigation::{
    folder_state::Order,
    history::{join, leaf_of, parent_of},
};
use std::rc::Rc;

pub const BASE: &str = "/fixture/group/a";
pub const GROUP: &str = "/fixture/group";
pub const FIXTURE: &str = "/fixture";
pub const OTHER: &str = "/fixture/other";
pub const ELSEWHERE: &str = "/elsewhere";
pub const ELSEWHERE_RAIL: &str = "/elsewhere/rail";
/// The probe's $HOME, which the sidebar rail's first row opens.
pub const RAIL: &str = "/home/probe";

fn folders(names: &[&str]) -> Vec<(String, bool)> {
    names
        .iter()
        .map(|name| ((*name).to_owned(), true))
        .collect()
}

/// makeDirs: every directory the probe lists or peeks, all rows folders except data.bin.
pub fn moves_dirs() -> Dirs {
    let mut dirs = Dirs::new();
    dirs.insert("/".into(), folders(&["fixture"]));
    dirs.insert(FIXTURE.into(), folders(&["group", "other"]));
    dirs.insert(GROUP.into(), folders(&["a", "b", "file"]));
    dirs.insert(ELSEWHERE.into(), folders(&["rail"]));
    dirs.insert(ELSEWHERE_RAIL.into(), folders(&["child", "second"]));
    let suffixes = [
        "",
        "/child",
        "/child/grand",
        "/child/grand/child",
        "/second",
        "/second/child",
    ];
    for path in [BASE, "/fixture/group/b", OTHER, ELSEWHERE_RAIL] {
        for suffix in suffixes {
            let names = if suffix == "/child" {
                ["grand", "second"]
            } else {
                ["child", "second"]
            };
            dirs.insert(format!("{path}{suffix}"), folders(&names));
        }
    }
    dirs.insert(
        "/fixture/group/file".into(),
        vec![("data.bin".to_owned(), false)],
    );
    dirs.insert(RAIL.into(), folders(&["child", "second"]));
    dirs.insert(format!("{RAIL}/child"), folders(&["grand", "second"]));
    let mut at = RAIL;
    while at != "/" {
        let parent = parent_of(at);
        dirs.entry(parent.to_owned())
            .or_insert_with(|| folders(&[leaf_of(at)]));
        at = parent;
    }
    dirs
}

/// A denied peek in wire shape: no rows, the directory's mode.
pub fn denied(path: &str, first: usize, mode: u32) -> Peeked {
    Peeked {
        path: path.to_owned(),
        hidden: false,
        hidden_last: false,
        first,
        order: String::new(),
        total: 0,
        facts: Facts {
            dev: 2049,
            writable: true,
        },
        denied: Some(mode),
        rows: Rc::from([]),
    }
}

/// What one rendered state draws: each shown column's row count left to right, and whether the third column shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Census {
    pub columns: Vec<usize>,
    pub third: bool,
}

impl Sim {
    /// The probe's window: 2100 by 600 at columnsLimit 3, over a pane that has listed nothing yet.
    pub fn moves() -> Self {
        let area = Area::new(2100., 600., Some(3.), 0., SETTINGS);
        Self::new(area, Pane::at("", Vec::new()), moves_dirs(), None)
    }

    /// Frames.census: every shown column, the peeked ones by their rows and the active one by the listing's.
    pub fn census(&self) -> Census {
        let columns = self
            .area
            .tracks()
            .iter()
            .filter_map(|view| match &view.rows {
                TrackRows::Peek(rows) => Some(rows.len()),
                TrackRows::Listing => Some(self.pane.rows.len()),
                TrackRows::Preview | TrackRows::Blank => None,
            })
            .collect();
        Census {
            columns,
            third: self.area.host.third_shown(self.area.settings),
        }
    }

    /// settled(): nothing out, the third column ready on the cursor's own row, and every shown column drawing rows.
    pub fn settled(&self) -> bool {
        let host = &self.area.host;
        let dir = self.pane.cursor_row().is_some_and(|row| row.dir);
        let shown = answered(&self.area, &self.pane, &host.shown.child);
        let ready = host.ready(shown, self.area.settings, self.p.preview.ready);
        if self.pane.in_flight || !self.queue.is_empty() || !ready || !host.shown.has_row {
            return false;
        }
        if dir && host.shown.child != child_of(&self.pane) {
            return false;
        }
        if !dir && !self.p.preview.ready {
            return false;
        }
        let census = self.census();
        census.columns.len() == self.area.count() - usize::from(!dir)
            && census.columns.iter().all(|rows| *rows > 0)
    }

    /// Frames until settled and at least this many, each state sampled before its frame; a phase past 300 frames stalls.
    pub fn settle(&mut self, c: &mut Checks, case: &str, phase: u32, frames: usize) -> Vec<Census> {
        let mut samples = Vec::new();
        let mut waited = 0;
        while waited <= 300 && (waited < frames || !self.settled()) {
            samples.push(self.census());
            self.frame();
            waited += 1;
        }
        c.ok(330, &format!("stalled {case} phase={phase}"), waited <= 300);
        samples
    }

    /// Event by event, a rendered frame at a time, until the condition holds; false after 300 frames.
    pub fn wait(&mut self, done: impl Fn(&Sim) -> bool) -> bool {
        for _ in 0..300 {
            if done(self) {
                return true;
            }
            let until = self.now + FRAME_MS;
            while self.step(until) {
                if done(self) {
                    return true;
                }
            }
            self.now = until;
            self.stand_in();
        }
        done(self)
    }

    /// Runs this many milliseconds a frame at a time, handing every event's state and every frame's to the reader.
    pub fn watch(&mut self, ms: u64, mut seen: impl FnMut(&Sim)) {
        let end = self.now + ms;
        while self.now < end {
            let until = (self.now + FRAME_MS).min(end);
            while self.step(until) {
                seen(self);
            }
            self.now = until;
            self.stand_in();
            seen(self);
        }
    }

    pub fn run(&mut self, ms: u64) {
        self.watch(ms, |_| {});
    }

    /// The listing has landed at this path.
    pub fn landed_at(&self, path: &str) -> bool {
        !self.pane.in_flight && self.pane.here == path
    }

    pub fn track(&self, slot: Slot) -> Option<TrackView> {
        self.area
            .tracks()
            .iter()
            .find(|view| view.slot == slot)
            .cloned()
    }

    /// A pointer press on a peek column's row: the folder it names, "" when the column or the row is missing.
    pub fn click(&self, c: &mut Checks, slot: Slot, index: usize) -> String {
        let track = self.track(slot);
        let rows = match track.as_ref().map(|view| &view.rows) {
            Some(TrackRows::Peek(rows)) => Some(Rc::clone(rows)),
            _ => None,
        };
        c.ok(141, "pointer target is missing", rows.is_some());
        let picked: Option<PeekRow> = rows.and_then(|rows| rows.get(index).cloned());
        c.ok(145, "pointer target has no row", picked.is_some());
        match (track, picked) {
            (Some(view), Some(row)) => join(&view.path, &row.name),
            _ => String::new(),
        }
    }

    fn key(&self, path: &str) -> String {
        peek_key(path, self.pane.hidden, self.pane.hidden_last)
    }

    /// rowsFor: the names a column draws, its landing stand-in first, at the pane's current path.
    pub fn rows_for(&self, path: &str) -> Vec<String> {
        let ordinary = self
            .area
            .peeks
            .rows(&self.key(path))
            .cloned()
            .unwrap_or_else(|| Rc::from([]));
        self.area
            .landing
            .rows(path, &self.pane.here, ordinary)
            .iter()
            .map(|row| row.name.clone())
            .collect()
    }

    /// deniedMode: None for a column whose last answer was a read.
    pub fn denied_mode(&self, path: &str) -> Option<u32> {
        self.area.peeks.denied(&self.key(path))
    }

    pub fn waiting(&self, path: &str) -> bool {
        self.area.landing.waiting(path, &self.pane.here)
    }

    pub fn loading(&self, path: &str) -> bool {
        self.area.landing.loading(path, &self.pane.here)
    }

    /// The backend's watch line for a directory.
    pub fn changed(&mut self, path: &str) {
        self.event += 1;
        let Self { area, pane, .. } = &mut *self;
        pane.with(|view| area.changed(path, &view.lens));
        self.drain();
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.event += 1;
        let now = self.now;
        let Self { area, pane, p, .. } = &mut *self;
        pane.with(|view| area.set_visible(visible, &view, &mut p.ports(), now));
        self.drain();
    }

    /// ViewState columnsLimit changed: the count re-lays and the area asks for what it now draws.
    pub fn set_limit(&mut self, limit: f64) {
        self.area.set_limit(Some(limit));
        self.sync();
    }

    /// A peek for this path is queued at the backend.
    pub fn queued_peek(&self, path: &str) -> bool {
        self.queue
            .iter()
            .any(|due| matches!(&due.ask, Ask::Peek { path: asked, .. } if asked == path))
    }

    /// Takes the queued peek for this path off the backend, answering its window size.
    pub fn take_peek(&mut self, path: &str) -> Option<usize> {
        let at = self
            .queue
            .iter()
            .position(|due| matches!(&due.ask, Ask::Peek { path: asked, .. } if asked == path))?;
        match self.queue.remove(at).ask {
            Ask::Peek { first, .. } => Some(first),
            Ask::List { .. } => None,
        }
    }

    /// The landing keeps a superseded landing, read from its debug form since the field is private.
    pub fn keeps_previous(&self) -> bool {
        format!("{:?}", self.area.landing).contains("previous: Some(")
    }

    /// The landing in flight targets this path, read from its debug form since the field is private.
    pub fn landing_target_is(&self, path: &str) -> bool {
        format!("{:?}", self.area.landing).contains(&format!("current: State {{ target: {path:?}"))
    }

    /// landing.begin(null, []) called directly, the way the probe's control does.
    pub fn begin_without_carry(&mut self) {
        let orders = |_: &str| -> Option<Order> { None };
        let lens = Lens {
            here: &self.pane.here,
            hidden: self.pane.hidden,
            hidden_last: self.pane.hidden_last,
            first: self.pane.first,
            orders: &orders,
        };
        self.area
            .landing
            .begin(None, &[], &[], &self.area.peeks, &lens, self.now);
    }
}
