//! tests/columnsfolder.qml driver and its first phases: a null first show, an unanswered folder's data wait, its landing and a file's hold.
use super::check::Checks;
use super::cols_moves::{answered, child_of};
use super::fixture::{reply_with, row, Pane, Producers, SETTINGS};
use flea::protocol::RowFacts;
use flea_gui::columns::{
    geometry::Slot,
    peeks::{peek_key, Landed},
    preview_host::Cursor,
    Area, TrackState, TrackView,
};
use flea_gui::navigation::history::join;
use std::rc::Rc;

pub const SOURCE: &str = "tests/columnsfolder.qml";
pub const STUDIO: &str = "/probe/studio";
/// The stub pane's windowSize, which every peek asks and answers under.
pub const WINDOW: usize = 35;

/// The probe's 1200 by 600 area over a stub pane whose peeks answer only when the probe sends a reply.
pub struct Probe {
    pub area: Area,
    pub pane: Pane,
    pub p: Producers,
    pub now: u64,
    /// The swap calls seen when the last move under test was made.
    pub mark: usize,
}

impl Probe {
    pub fn start() -> Self {
        let area = Area::new(1200., 600., None, 0., SETTINGS);
        let mut pane = Pane::at(STUDIO, Vec::new());
        pane.first = WINDOW;
        let mut probe = Self {
            area,
            pane,
            p: Producers::default(),
            now: 0,
            mark: 0,
        };
        probe.sync();
        probe
    }

    pub fn sync(&mut self) {
        let now = self.now;
        let Self { area, pane, p, .. } = &mut *self;
        pane.with(|view| area.sync(&view, &mut p.ports(), now));
    }

    pub fn tick(&mut self) {
        let now = self.now;
        let Self { area, pane, p, .. } = &mut *self;
        pane.with(|view| area.tick(&view, &mut p.ports(), now));
    }

    /// Runs every timer due by this time, in order.
    pub fn run_to(&mut self, until: u64) {
        while let Some(at) = self.area.next_deadline().filter(|at| *at <= until) {
            self.now = self.now.max(at);
            self.tick();
        }
        self.now = self.now.max(until);
    }

    /// The stub pane's rows replaced under an unmoved cursor index.
    pub fn rows(&mut self, rows: Vec<RowFacts>) {
        self.pane.rows = rows;
        self.pane.listing += 1;
        self.sync();
    }

    pub fn cursor(&mut self, at: usize) {
        self.pane.cursor = at;
        self.sync();
    }

    /// childPath(): the cursor folder, else the first phase's sub.
    pub fn child_path(&self) -> String {
        let child = child_of(&self.pane);
        if child.is_empty() {
            join(STUDIO, "sub")
        } else {
            child
        }
    }

    /// answerPeek(): one reply for childPath() on the area's reply path.
    pub fn answer(&mut self, rows: &[(&str, bool)]) -> Landed {
        let reply = reply_with(&self.child_path(), rows, false, false, WINDOW, "");
        let now = self.now;
        let Self { area, pane, p, .. } = &mut *self;
        pane.with(|view| area.peeked(&reply, &view, &mut p.ports(), now))
    }

    /// The swap's hold captured its picture, or its burst ended: the queued showCursorRow runs for the cursor now.
    pub fn captured(&mut self) {
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

    /// The swap released its hold: the file's cap and minimum expired with its meta unanswered.
    pub fn release(&mut self) {
        self.p.swap.holding = false;
    }

    pub fn answered(&self, path: &str) -> bool {
        answered(&self.area, &self.pane, path)
    }

    /// assertIdle(): the third swap holds no picture.
    pub fn idle(&self) -> bool {
        !self.p.swap.holding
    }

    pub fn mark(&mut self) {
        self.mark = self.p.swap.calls.len();
    }

    /// assertHeld(): the live picture is kept, the swap still holding with nothing cancelled since the mark.
    pub fn held(&self) -> bool {
        self.p.swap.holding
            && !self.p.swap.calls[self.mark..]
                .iter()
                .any(|call| call == "cancel")
    }

    /// rowsFor(): the names a column draws at the pane's path.
    pub fn rows_for(&self, path: &str) -> Vec<String> {
        let key = peek_key(path, self.pane.hidden, self.pane.hidden_last);
        let ordinary = self
            .area
            .peeks
            .rows(&key)
            .cloned()
            .unwrap_or_else(|| Rc::from([]));
        self.area
            .landing
            .rows(path, &self.pane.here, ordinary)
            .iter()
            .map(|row| row.name.clone())
            .collect()
    }

    pub fn child_track(&self) -> Option<TrackView> {
        self.area
            .tracks()
            .iter()
            .find(|view| view.slot == Slot::Child)
            .cloned()
    }

    /// The file preview draws in the third column.
    pub fn preview_visible(&self) -> bool {
        self.area.host.preview_visible(self.area.settings)
    }
}

/// a) the launch-shaped null first show, then the cursor lands on an unanswered folder.
pub fn phase_null(c: &mut Checks, probe: &mut Probe) {
    probe.run_to(800);
    let shown = &probe.area.host.shown;
    c.eq(
        168,
        "a) a null first show holds a row, want none",
        shown.has_row,
        false,
    );
    c.ok(
        171,
        "a) a null first show leaves the third swap holding, want idle",
        probe.idle(),
    );
    probe.rows(vec![row("sub", true)]);
    let shown = &probe.area.host.shown;
    c.eq(
        174,
        "a) an unanswered folder shows at once, want the old column kept",
        shown.has_row,
        false,
    );
    c.eq(
        176,
        "a) an unanswered folder marks dir, want the old column kept",
        shown.is_dir,
        false,
    );
    c.ok(
        179,
        "a) an unanswered folder wait leaves the third swap holding, want idle",
        probe.idle(),
    );
}

/// a) the peek lands after the 150 ms fallback showed sub pending; d) the empty landing settles its tile.
pub fn phase_empty(c: &mut Checks, probe: &mut Probe) {
    probe.run_to(1100);
    probe.answer(&[]);
    let sub = join(STUDIO, "sub");
    let shown = probe.area.host.shown.clone();
    c.eq(
        188,
        "a) a landed peek keeps the old column, want the folder",
        shown.is_dir,
        true,
    );
    c.eq(
        190,
        "a) a landed peek shows <shownChildPath>, want <childPath()>",
        shown.child,
        probe.child_path(),
    );
    c.ok(
        191,
        "a) a landed peek leaves the third swap holding, want idle",
        probe.idle(),
    );
    // The intent of the tile check: the landed empty folder draws its empty tile, not rows and not a lock.
    c.eq(
        194,
        "d) an empty folder draws no empty tile",
        probe.child_track().map(|view| (view.path, view.state)),
        Some((sub.clone(), TrackState::Empty)),
    );
    c.eq(
        197,
        "d) a data-held empty landing runs its entrance, want it settled",
        probe.area.hero_settled.clone(),
        Some(sub),
    );
}

/// The file setup: a.txt's hold captures, shows the file and expires with its meta unanswered.
pub fn phase_file(c: &mut Checks, probe: &mut Probe) {
    probe.pane.cursor = 0;
    probe.rows(vec![
        row("a.txt", false),
        row("b.png", false),
        row("late", true),
    ]);
    probe.run_to(1116);
    probe.captured();
    probe.run_to(1300);
    probe.release();
    probe.run_to(1950);
    c.eq(
        216,
        "file setup shows a folder, want the file",
        probe.area.host.shown.is_dir,
        false,
    );
}

/// b) a file move holds a live picture; j onto the unanswered late hands that picture to the folder's wait.
pub fn phase_b(c: &mut Checks, probe: &mut Probe) {
    probe.cursor(1);
    // The preview's replace holds for b.png, and its frame captures by the next poll.
    probe.run_to(1966);
    probe.p.swap.holding = true;
    let late = join(STUDIO, "late");
    c.eq(
        240,
        "b) late answers before the move, want it unanswered",
        probe.answered(&late),
        false,
    );
    probe.mark();
    probe.cursor(2);
    probe.run_to(2086);
    c.ok(
        247,
        "b) j onto an unanswered folder leaves the third swap idle, want the live picture kept",
        probe.held(),
    );
    c.eq(
        251,
        "b) an unanswered folder shows at once, want the old column kept",
        probe.area.host.shown.is_dir,
        false,
    );
    let child = probe.child_path();
    c.eq(
        253,
        "b) the waiting folder answers before its peek, want it unanswered",
        probe.answered(&child),
        false,
    );
    probe.answer(&[("kid.txt", false)]);
    c.eq(
        258,
        "b) a landed peek leaves late unanswered, want it answered",
        probe.answered(&child),
        true,
    );
    probe.run_to(2336);
    let shown = probe.area.host.shown.clone();
    c.eq(
        264,
        "b) a landed folder peek keeps the old column, want the folder",
        shown.is_dir,
        true,
    );
    c.eq(
        266,
        "b) a landed folder peek shows <shownChildPath>, want <childPath()>",
        shown.child,
        probe.child_path(),
    );
    c.eq(
        268,
        "b) a landed folder keeps the preview visible, want it hidden",
        probe.preview_visible(),
        false,
    );
    c.ok(
        271,
        "b) a landed folder peek leaves the third swap holding, want idle",
        probe.idle(),
    );
}

/// c) back on the file: a.txt holds, captures and expires again.
pub fn phase_c(c: &mut Checks, probe: &mut Probe) {
    probe.cursor(0);
    probe.run_to(2352);
    probe.captured();
    probe.run_to(2500);
    probe.release();
    probe.run_to(3136);
    c.eq(
        284,
        "c) back on the file keeps the folder, want the file",
        probe.area.host.shown.is_dir,
        false,
    );
}
