//! TrashView.qml as a state machine: an integer total, a held metadata window, marks and one request in flight.
use super::rows::{Line, TrashRow};
use super::selection::{Extend, Selection};
use super::thumbs::{self, Held, Policy, Thumbs};
use super::words::{Arm, Snapshot};

/// The most rows one window request asks for, the backend's own ceiling.
pub const WINDOW_CAP: usize = 350;
/// GIO reports top-level changes; an open confirmation also checks nested identities this often.
pub const CHECK_MS: u64 = 2500;

/// Where a context menu opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    /// Under this row at the row padding, the way the listing's own menu opens.
    Row(usize),
    /// The top centre of the view, under the strip.
    Top,
    /// Where the pointer pressed, in view coordinates.
    Point(f32, f32),
}

/// What the view tells its host, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Signal {
    Status { text: String, error: bool },
    Arm(String),
    Result { headline: String, detail: String, error: bool },
    Count(usize),
    Back,
    FocusRail,
    Action(String),
    Context { at: Anchor, selection: bool },
    /// Time a thumbnail settle: call settled(generation) after delay_ms.
    Settle { generation: u64, delay_ms: u64 },
}

/// A key the view reads: an action from the generated table, or the two raw keys the oracle reads itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press<'a> {
    Action(&'a str),
    Backspace,
    F5,
}

/// The keys the dd prompt names, from the running preset.
#[derive(Clone, Copy, Debug, Default)]
pub struct Keys<'a> {
    pub arm: &'a str,
    pub alone: &'a str,
}

#[derive(Debug, Default)]
pub struct View {
    pub opened: bool,
    /// Every ancestor is shown; a hidden one makes the view unready for thumbnails.
    pub shown: bool,
    pub total: usize,
    pub bytes: Option<(u64, bool)>,
    pub first: usize,
    pub rows: Vec<TrashRow>,
    pub selection: Selection,
    pub cursor: usize,
    pub arm: Arm,
    pub request_id: u64,
    pub pending_op: String,
    pub operation_active: bool,
    pub refresh_pending: bool,
    pub confirming: bool,
    pub initial_action: String,
    pub(super) pending_selection: Option<(usize, Extend)>,
    pub landing: bool,
    pub error_text: Option<String>,
    pub(super) review: Review,
    /// The open delete card, filled by the backend's prepare reply.
    pub confirmation: Option<Snapshot>,
    /// Keyboard focus sits on Empty Trash… in the strip.
    pub strip_focus: bool,
    pub scroll_y: f32,
    pub height: f32,
    pub row_height: f32,
    pub thumbs: Thumbs,
    pub policy: Policy,
    was_ready: bool,
    out: Vec<String>,
    pub(super) signals: Vec<Signal>,
}

// What the next prepare re-reviews after the Trash changed under an open card.
#[derive(Debug, Default)]
pub(super) struct Review {
    pub(super) all: bool,
    pub(super) selection_all: bool,
    pub(super) selection_token: u64,
    pub(super) uris: Vec<String>,
    pub(super) excluded: Vec<String>,
    pub(super) identities: Vec<String>,
    pub(super) token: u64,
}

impl View {
    pub fn new(row_height: f32) -> Self {
        Self { shown: true, row_height, ..Self::default() }
    }

    /// Request lines in send order, thumbnail lines included.
    pub fn drain(&mut self) -> Vec<String> {
        std::mem::take(&mut self.out)
    }
    pub fn signals(&mut self) -> Vec<Signal> {
        std::mem::take(&mut self.signals)
    }
    pub fn busy(&self) -> bool {
        !self.pending_op.is_empty()
    }
    pub fn ready(&self) -> bool {
        self.opened && self.shown && !self.operation_active && self.pending_op != "list"
    }
    /// The rows the listing shows at its height, and one more past each edge.
    pub(super) fn page(&self) -> usize {
        (self.height / self.row_height).ceil() as usize + 2
    }
    // Held rows past each edge of the view, so a wheel notch lands on rows already read.
    fn margin(&self) -> usize {
        self.page().min(WINDOW_CAP.saturating_sub(self.page()) / 2)
    }
    pub fn window_rows(&self) -> usize {
        (self.page() + 2 * self.margin()).clamp(1, WINDOW_CAP)
    }
    // The first row the viewport shows and the row after its last.
    fn visible(&self) -> (usize, usize) {
        let top = (self.scroll_y / self.row_height).floor() as usize;
        (top, self.total.min(((self.scroll_y + self.height) / self.row_height).ceil() as usize))
    }
    /// Every row the viewport shows is held, so it draws whole while a window reads.
    pub fn covers_viewport(&self) -> bool {
        let (top, end) = self.visible();
        top >= self.first && end <= self.first + self.rows.len()
    }
    pub fn row_at(&self, index: usize) -> Option<&TrashRow> {
        index.checked_sub(self.first).and_then(|at| self.rows.get(at))
    }
    pub fn is_selected(&self, uri: &str) -> bool {
        self.selection.is_selected(uri, &self.rows)
    }
    pub fn selected_count(&self) -> usize {
        self.selection.len()
    }
    pub fn held(&self) -> Held<'_> {
        Held { first: self.first, rows: &self.rows, total: self.total }
    }
    pub fn span(&self) -> Option<(usize, usize)> {
        thumbs::viewport(self.scroll_y, self.row_height, self.height, self.total)
    }

    pub(super) fn flush(&mut self, settle: Option<(u64, u64)>) {
        self.out.extend(self.thumbs.drain());
        if let Some((generation, delay_ms)) = settle {
            self.signals.push(Signal::Settle { generation, delay_ms });
        }
    }

    // TrashThumbs.onReadyChanged, run after every entry point.
    pub(super) fn settle_ready(&mut self) {
        let ready = self.ready();
        if ready != self.was_ready {
            self.was_ready = ready;
            let settle = if ready { self.thumbs.restart(true) } else { self.thumbs.reset(); None };
            self.flush(settle);
        }
    }

    pub(super) fn set_rows(&mut self, first: usize, rows: Vec<TrashRow>) {
        self.first = first;
        self.rows = rows;
        let ready = self.ready();
        let settle = self.thumbs.sync(Held { first: self.first, rows: &self.rows, total: self.total }, ready);
        self.flush(settle);
    }

    pub(super) fn send(&mut self, op: &str, line: Line) {
        self.request_id += 1;
        self.pending_op = op.into();
        if op == "restore" || op == "delete" {
            self.operation_active = true;
        }
        if op == "list" || op == "restore" || op == "delete" {
            self.thumbs.reset();
            self.flush(None);
        }
        self.out.push(line.text("op", op).number("id", self.request_id).finish());
        self.settle_ready();
    }
    pub(super) fn browse() -> Line {
        Line::new("trashbrowse")
    }

    /// The settle timer fired for this generation.
    pub fn settled(&mut self, generation: u64) {
        let (span, ready, policy) = (self.span(), self.ready(), self.policy);
        let held = Held { first: self.first, rows: &self.rows, total: self.total };
        self.thumbs.request(generation, held, span, policy, ready);
        self.flush(None);
    }

    /// A trashthumbed reply.
    pub fn thumbnailed(&mut self, id: u64, file: &str) -> bool {
        let ready = self.ready();
        let held = Held { first: self.first, rows: &self.rows, total: self.total };
        let taken = self.thumbs.receive(id, file, held, ready);
        self.flush(None);
        taken
    }

    pub fn set_policy(&mut self, policy: Policy) {
        if policy == self.policy {
            return;
        }
        self.policy = policy;
        self.thumbs.reset();
        let settle = self.thumbs.restart(self.ready());
        self.flush(settle);
    }

    pub fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
        self.settle_ready();
    }

    /// The listing scrolled or resized: fetch a window that holds it and cancel thumbnails that left it.
    pub fn set_viewport(&mut self, scroll_y: f32, height: f32) {
        let max = (self.total as f32 * self.row_height - height).max(0.);
        self.scroll_y = scroll_y.clamp(0., max);
        self.height = height;
        self.ensure_window();
        let (span, ready) = (self.span(), self.ready());
        let settle = self.thumbs.scroll(span, ready);
        self.flush(settle);
    }

    // ListView.positionViewAtIndex(index, Contain).
    pub(super) fn contain(&mut self, index: usize) {
        let top = index as f32 * self.row_height;
        let mut y = self.scroll_y;
        if top < y {
            y = top;
        } else if top + self.row_height > y + self.height {
            y = top + self.row_height - self.height;
        }
        if y != self.scroll_y {
            self.set_viewport(y, self.height);
        }
    }

    pub fn open(&mut self, action: &str) {
        self.arm.clear();
        self.opened = true;
        self.strip_focus = false;
        if self.operation_active {
            self.settle_ready();
            return;
        }
        self.selection.clear();
        self.cursor = 0;
        self.total = 0;
        self.set_rows(0, Vec::new());
        self.error_text = None;
        self.bytes = None;
        self.landing = true;
        self.confirming = false;
        self.confirmation = None;
        self.refresh_pending = false;
        self.initial_action = action.into();
        let count = self.window_rows();
        self.send("list", Self::browse().number("start", 0).number("count", count).flag("recover", true));
    }

    pub fn close(&mut self) {
        self.arm.clear();
        self.opened = false;
        self.confirming = false;
        self.confirmation = None;
        self.initial_action.clear();
        self.settle_ready();
        if !self.operation_active {
            self.send("cancel", Self::browse().flag("clearSelection", true));
        }
        self.signals.push(Signal::Back);
    }

    pub fn refresh(&mut self) {
        if !self.opened {
            return;
        }
        if self.busy() {
            self.refresh_pending = true;
            return;
        }
        if self.confirming {
            return self.source_changed();
        }
        self.refresh_pending = false;
        self.bytes = None;
        let (first, count, keys) = (self.first, self.window_rows(), self.selection.keys());
        self.send("list", Self::browse().number("start", first).number("count", count).texts("uris", &keys));
    }

    /// The Trash changed under the view: re-review an open card, or re-read the listing.
    pub fn source_changed(&mut self) {
        if !self.opened {
            return;
        }
        self.arm.clear();
        self.confirmation = None;
        if self.busy() {
            self.refresh_pending = true;
            return;
        }
        self.refresh_pending = false;
        if !self.confirming {
            return self.refresh();
        }
        let r = &self.review;
        let line = Self::browse().flag("all", r.selection_all).flag("emptyTrash", r.all).texts("uris", &r.uris)
            .texts("exclude", &r.excluded).texts("identities", &r.identities)
            .number("selectionToken", r.selection_token).number("refreshToken", r.token);
        self.send("prepare", line);
    }

    pub fn ensure_window(&mut self) {
        if !self.opened || self.total == 0 || self.busy() || self.confirmation.is_some() {
            return;
        }
        let (top, end) = self.visible();
        let (held_end, margin) = (self.first + self.rows.len(), self.margin());
        // Read the next window while the view is still half a margin inside the held rows.
        let near_top = self.first > 0 && top < self.first + margin / 2;
        let near_end = held_end < self.total && end + margin / 2 > held_end;
        if top < self.first || end > held_end || near_top || near_end {
            let (start, count) = (top.saturating_sub(margin), self.window_rows());
            self.send("window", Self::browse().number("start", start).number("count", count));
        }
    }
}
