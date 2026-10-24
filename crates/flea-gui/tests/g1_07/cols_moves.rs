//! Driver for tests/columnsmoves.qml and tests/columnsback.qml: a pane over a directory map whose backend replies by frame or serially.
use super::fixture::{reply_with, row, Pane, Producers};
use flea::protocol::RowFacts;
use flea_gui::columns::{
    carry::{Leaving, ListOrder},
    peeks::{peek_key, Facts, Landed, Peeked},
    preview_host::Cursor,
    Area,
};
use flea_gui::navigation::history::join;
use std::collections::HashMap;

/// One rendered frame of the probe's frame-driven backend.
pub const FRAME_MS: u64 = 16;
/// The listing order a peek sorts in, so a carried seed matches it.
pub const NAME_ORDER: ListOrder<'static> = ListOrder {
    by: "name",
    desc: false,
    folders_first: true,
    group_by_kind: false,
};

/// Each directory's rows by name and folder flag, in listing order.
pub type Dirs = HashMap<String, Vec<(String, bool)>>;

/// One request the backend holds until it is due.
pub enum Ask {
    Peek {
        path: String,
        hidden: bool,
        hidden_last: bool,
        first: usize,
        order: String,
    },
    List {
        path: String,
        select: String,
    },
}

pub struct Due {
    pub at: u64,
    pub ask: Ask,
}

/// The pane, the area and its producers, with a backend that answers from the directory map.
pub struct Sim {
    pub area: Area,
    pub pane: Pane,
    pub p: Producers,
    pub dirs: Dirs,
    pub now: u64,
    /// The listing's total, which the carry's seed count reads.
    pub total: usize,
    /// The rows a listing holds of its directory.
    pub window: usize,
    /// None replies a list one frame and a peek two frames later; Some(ms) replies in order, each ms after the last.
    pub serial: Option<u64>,
    pub free_at: u64,
    pub queue: Vec<Due>,
    pub freeze_peeks: bool,
    pub freeze_lists: bool,
    /// Every event the area handled, counted, so a peek is placed before, at or after a landing.
    pub event: usize,
    /// Each peek sent, by path and the event that sent it.
    pub asked: Vec<(String, usize)>,
    /// The event of each listing that landed.
    pub landings: Vec<usize>,
    /// A hold taken last frame, whose picture completes at the end of this one.
    pub capture_due: bool,
    /// The listing and cursor index whose file the preview stand-in loaded.
    pub loaded: Option<(String, usize)>,
}

/// The cursor folder's full path, "" for a file or no row.
pub fn child_of(pane: &Pane) -> String {
    pane.cursor_row()
        .filter(|row| row.dir)
        .map(|row| join(&pane.base, &row.name))
        .unwrap_or_default()
}

/// ColumnsArea.answered: a reply has landed for the path under the pane's key.
pub fn answered(area: &Area, pane: &Pane, path: &str) -> bool {
    area.peeks
        .answered(&peek_key(path, pane.hidden, pane.hidden_last), path)
}

impl Sim {
    pub fn new(area: Area, pane: Pane, dirs: Dirs, serial: Option<u64>) -> Self {
        let mut sim = Self {
            area,
            total: pane.rows.len(),
            window: pane.first,
            pane,
            p: Producers::default(),
            dirs,
            now: 0,
            serial,
            free_at: 0,
            queue: Vec::new(),
            freeze_peeks: false,
            freeze_lists: false,
            event: 0,
            asked: Vec::new(),
            landings: Vec::new(),
            capture_due: false,
            loaded: None,
        };
        sim.sync();
        sim
    }

    fn enqueue(&mut self, ask: Ask, frames: u64) {
        let at = match self.serial {
            Some(ms) => {
                let at = self.free_at.max(self.now) + ms;
                self.free_at = at;
                at
            }
            None => self.now + frames * FRAME_MS,
        };
        self.queue.push(Due { at, ask });
    }

    /// Every peek line the area queued goes to the backend, a peek replying two frames later.
    pub fn drain(&mut self) {
        let lines: Vec<String> = std::mem::take(&mut self.area.outbox);
        for line in lines {
            let doc = flea::jsondoc::parse(&line).expect("peek line is JSON");
            let text = |key: &str| {
                doc.get(key)
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_owned()
            };
            let flag = |key: &str| doc.get(key).and_then(|value| value.as_bool()) == Some(true);
            let first = doc
                .get("first")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.) as usize;
            let by = text("by");
            let order = if by.is_empty() {
                String::new()
            } else {
                format!(",\"by\":\"{by}\",\"desc\":{}", flag("desc"))
            };
            let path = text("path");
            self.asked.push((path.clone(), self.event));
            let ask = Ask::Peek {
                path,
                hidden: flag("hidden"),
                hidden_last: flag("hiddenLast"),
                first,
                order,
            };
            self.enqueue(ask, 2);
        }
    }

    /// The one entry after any pane change.
    pub fn sync(&mut self) {
        self.event += 1;
        let now = self.now;
        let Self { area, pane, p, .. } = &mut *self;
        pane.with(|view| area.sync(&view, &mut p.ports(), now));
        self.drain();
    }

    pub fn tick(&mut self) {
        self.event += 1;
        let now = self.now;
        let Self { area, pane, p, .. } = &mut *self;
        pane.with(|view| area.tick(&view, &mut p.ports(), now));
        // This model fixture draws each timer step immediately; real-window probes exercise delayed painting.
        let tracks = std::rc::Rc::clone(area.tracks());
        for track in tracks
            .iter()
            .filter(|track| track.state == flea_gui::columns::TrackState::Loading)
        {
            area.landing.mark_shown(&track.path, true, &pane.here, now);
        }
        self.drain();
    }

    /// One reply on the area's reply path.
    pub fn peek(&mut self, reply: &Peeked) -> Landed {
        self.event += 1;
        let now = self.now;
        let Self { area, pane, p, .. } = &mut *self;
        let landed = pane.with(|view| area.peeked(reply, &view, &mut p.ports(), now));
        self.drain();
        landed
    }

    /// The pane's own navigation: a listing still out answers into nothing, the area plans the carry and pre-asks, then the list goes out.
    pub fn open(&mut self, target: &str, select: &str) {
        self.event += 1;
        self.queue
            .retain(|due| !matches!(due.ask, Ask::List { .. }));
        self.pane.in_flight = true;
        let now = self.now;
        let total = self.total;
        let Self { area, pane, .. } = &mut *self;
        let pane: &Pane = pane;
        pane.with(|view| {
            let leaving = Leaving {
                here: view.lens.here,
                target,
                tag: "",
                window_size: pane.first,
                total,
                held_start: 0,
                narrowed: false,
                rows: &pane.rows,
                order: NAME_ORDER,
                facts: Facts {
                    dev: 2049,
                    writable: true,
                },
            };
            area.listing_requested(leaving, &view.lens, now);
        });
        self.drain();
        let list = Ask::List {
            path: target.to_owned(),
            select: select.to_owned(),
        };
        self.enqueue(list, 1);
        self.sync();
    }

    /// The listing lands whole in one turn: rows and path, the cursor on the selected row, then the flight's end.
    fn land(&mut self, path: &str, select: &str) {
        self.event += 1;
        self.landings.push(self.event);
        let all = self.dirs.get(path).cloned().unwrap_or_default();
        self.total = all.len();
        let rows: Vec<RowFacts> = all
            .iter()
            .take(self.window)
            .map(|(name, dir)| row(name, *dir))
            .collect();
        self.pane.go(path, rows);
        let picked = self
            .pane
            .rows
            .iter()
            .position(|row| !select.is_empty() && join(path, &row.name) == select);
        if let Some(at) = picked {
            self.pane.cursor = at;
        }
        let now = self.now;
        let Self { area, pane, p, .. } = &mut *self;
        pane.with(|view| area.path_landed(&view, &mut p.ports(), now));
        pane.in_flight = false;
        pane.with(|view| area.flight_ended(&view, &mut p.ports(), now));
        self.drain();
    }

    fn deliver(&mut self, ask: Ask) {
        match ask {
            Ask::Peek {
                path,
                hidden,
                hidden_last,
                first,
                order,
            } => {
                let rows = self.dirs.get(&path).cloned().unwrap_or_default();
                let named: Vec<(&str, bool)> = rows
                    .iter()
                    .take(first)
                    .map(|(name, dir)| (name.as_str(), *dir))
                    .collect();
                let reply = reply_with(&path, &named, hidden, hidden_last, first, &order);
                self.peek(&reply);
            }
            Ask::List { path, select } => self.land(&path, &select),
        }
    }

    fn blocked(&self, ask: &Ask) -> bool {
        match ask {
            Ask::Peek { .. } => self.freeze_peeks,
            Ask::List { .. } => self.freeze_lists,
        }
    }

    /// Runs the earliest timer or reply due by this time; false when nothing is due.
    pub fn step(&mut self, until: u64) -> bool {
        let deadline = self.area.next_deadline().filter(|at| *at <= until);
        let due = self
            .queue
            .iter()
            .enumerate()
            .filter(|(_, due)| due.at <= until && !self.blocked(&due.ask))
            .min_by_key(|(index, due)| (due.at, *index))
            .map(|(index, due)| (index, due.at));
        match (deadline, due) {
            (None, None) => false,
            (Some(at), Some((_, due_at))) if at <= due_at => {
                self.now = self.now.max(at);
                self.tick();
                true
            }
            (Some(at), None) => {
                self.now = self.now.max(at);
                self.tick();
                true
            }
            (_, Some((index, at))) => {
                self.now = self.now.max(at);
                let due = self.queue.remove(index);
                self.deliver(due.ask);
                true
            }
        }
    }

    pub fn advance_to(&mut self, until: u64) {
        let mut guard = 0;
        while self.step(until) {
            guard += 1;
            assert!(guard < 100_000, "the timers never settle by {until}");
        }
        self.now = self.now.max(until);
    }

    /// The swap's capture completed or its burst ended the hold: the queued showCursorRow runs for the cursor as it is now.
    pub fn show(&mut self) {
        let child = child_of(&self.pane);
        let answered = answered(&self.area, &self.pane, &child);
        let row = self.pane.cursor_row();
        let cursor = Cursor {
            row: row.is_some(),
            is_dir: row.is_some_and(|row| row.dir),
            child: &child,
        };
        self.area.host.show_cursor_row(cursor, answered, false);
        self.sync();
    }

    /// Stands in for the G2 swap and preview at a frame's end: a hold completes a frame later and shows the row, a file loading ready.
    pub fn stand_in(&mut self) {
        let row = self.pane.cursor_row();
        let file = row.is_some_and(|row| !row.dir);
        let current = Some((self.pane.here.clone(), self.pane.cursor));
        self.p.preview.ready = file && self.loaded == current;
        // An unanswered folder's wait keeps the picture until its reply or its fallback, both of which cancel it.
        let waits = row.is_some_and(|row| row.dir)
            && !answered(&self.area, &self.pane, &child_of(&self.pane));
        if !self.p.swap.holding || waits {
            self.capture_due = false;
            return;
        }
        if !self.capture_due {
            self.capture_due = true;
            return;
        }
        self.capture_due = false;
        self.p.swap.holding = false;
        if file {
            self.loaded = current;
            self.p.preview.ready = true;
        }
        self.show();
    }

    /// One rendered frame: the replies and timers due within it, then the swap stand-in.
    pub fn frame(&mut self) {
        let until = self.now + FRAME_MS;
        self.advance_to(until);
        self.stand_in();
    }
}
