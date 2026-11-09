//! Carry: X lifts the marked rows, or the cursor row, as one captured payload that rides until a click lands it, esc drops it or its source window or pane ends; no I/O, the rows resolve through the backend's paths answer as a copy's do, and the landing's verb and refusal are the drag's own (flea::ui_logic_drag_drop through the GUI's decide).
use super::wire::paths_request_line;
use super::words;
use super::Said;
use crate::i18n::I18n;

/// The status line while a carry rides, and its muted hint.
pub const STATUS: &str = "carry.status";
pub const HINT: &str = "carry.hint";

/// Where a carry was lifted: the window and pane side that asked, and the listing its row indices named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub window: u64,
    pub side: usize,
    pub folder: String,
    pub listing: u64,
}

/// A riding carry: the absolute paths captured at the lift, never re-read from a cursor or selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lifted {
    /// The source listing's row indices the paths answer named, in order.
    pub rows: Vec<usize>,
    pub paths: Vec<String>,
    pub source: Source,
    pub serial: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Resolving(Source, Vec<usize>, u64),
    Draining(Source),
    Riding(Lifted),
}

/// What a lift asks of the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LiftOut {
    /// Send this paths request on the source pane's backend; the answer comes back through `resolved`.
    Ask(String),
    /// The shared paths slot is held by another asker; say this and lift nothing.
    Busy(Said),
    /// No rows: the caller says its no-target line.
    Nothing,
}

/// What a paths answer was to the carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Not the carry's: another asker on this window side owns it.
    Foreign,
    /// The carry's own answer to a lift that was cancelled: consumed here, nothing rides.
    Drained,
    /// The named rows ride.
    Rides,
    /// The listing moved on under the ask, or the answer was empty: the stale line, as a copy's.
    Stale(Said),
}

/// One process's carry; at most one rides at a time, and a new lift replaces the old one whole.
#[derive(Clone, Debug, Default)]
pub struct Carry {
    phase: Phase,
    serial: u64,
}

impl Carry {
    pub fn new() -> Self {
        Self::default()
    }

    // Sample input: rows [2, 3, 4] on a free slot ask {"c":"paths","rows":[2,3,4]} and ride once the backend names them.
    pub fn lift(
        &mut self,
        i18n: &I18n,
        rows: &[usize],
        source: Source,
        slot_busy: bool,
    ) -> LiftOut {
        if rows.is_empty() {
            return LiftOut::Nothing;
        }
        if slot_busy || matches!(self.phase, Phase::Resolving(..) | Phase::Draining(_)) {
            return LiftOut::Busy(Said {
                text: words::resolving(i18n),
                error: false,
                paste_hint: false,
            });
        }
        self.serial += 1;
        self.phase = Phase::Resolving(source, rows.to_vec(), self.serial);
        LiftOut::Ask(paths_request_line(rows))
    }

    /// Whether this window side's backend owes the carry its paths answer.
    pub fn pending(&self, window: u64, side: usize) -> bool {
        matches!(&self.phase, Phase::Resolving(source, ..) | Phase::Draining(source) if source.window == window && source.side == side)
    }

    // The backend named the rows; a drained answer is told apart from a foreign one, so the caller never hands the carry's answer on.
    pub fn resolved(
        &mut self,
        i18n: &I18n,
        window: u64,
        side: usize,
        paths: Vec<String>,
        listing_now: u64,
    ) -> Answer {
        if !self.pending(window, side) {
            return Answer::Foreign;
        }
        let Phase::Resolving(source, rows, serial) = std::mem::take(&mut self.phase) else {
            return Answer::Drained;
        };
        if source.listing != listing_now || paths.is_empty() {
            return Answer::Stale(Said {
                text: words::stale(i18n),
                error: false,
                paste_hint: false,
            });
        }
        self.phase = Phase::Riding(Lifted {
            rows,
            paths,
            source,
            serial,
        });
        Answer::Rides
    }

    /// A refused or failed paths request ends a pending lift; true when it was the carry's.
    pub fn failed(&mut self, window: u64, side: usize) -> bool {
        let ours = self.pending(window, side);
        if ours {
            self.phase = Phase::Idle;
        }
        ours
    }

    pub fn riding(&self) -> Option<&Lifted> {
        match &self.phase {
            Phase::Riding(lifted) => Some(lifted),
            _ => None,
        }
    }

    pub fn active(&self) -> bool {
        matches!(self.phase, Phase::Resolving(..) | Phase::Riding(_))
    }

    // Sample output for 3 paths: ("Carrying 3 items", "click where they go, esc cancels").
    pub fn status(&self, i18n: &I18n) -> Option<(String, String)> {
        let lifted = self.riding()?;
        Some((i18n.trn(STATUS, lifted.paths.len() as i64), i18n.tr(HINT)))
    }

    /// Esc, or any lifecycle end the caller names: drops whatever is lifted or resolving; true when something was.
    pub fn cancel(&mut self) -> bool {
        let was = self.active();
        self.phase = match std::mem::take(&mut self.phase) {
            Phase::Resolving(source, ..) | Phase::Draining(source) => Phase::Draining(source),
            _ => Phase::Idle,
        };
        was
    }

    /// The source window closed or left its workspace: a riding carry ends, an owed answer stays claimed until it lands; true when the carry was that window's.
    pub fn end_window(&mut self, window: u64) -> bool {
        let ours = match &self.phase {
            Phase::Idle => false,
            Phase::Resolving(source, ..) | Phase::Draining(source) => source.window == window,
            Phase::Riding(lifted) => lifted.source.window == window,
        };
        if ours {
            self.phase = match std::mem::take(&mut self.phase) {
                Phase::Resolving(source, ..) | Phase::Draining(source) => Phase::Draining(source),
                _ => Phase::Idle,
            };
        }
        ours
    }

    /// The source pane's backend ended: its rows can no longer be named or moved, so the carry ends.
    pub fn end_source(&mut self, window: u64, side: usize) -> bool {
        let ours = match &self.phase {
            Phase::Idle => false,
            Phase::Resolving(source, ..) | Phase::Draining(source) => {
                source.window == window && source.side == side
            }
            Phase::Riding(lifted) => lifted.source.window == window && lifted.source.side == side,
        };
        if ours {
            self.phase = Phase::Idle;
        }
        ours
    }

    /// The transfer authority took the ask: the carry that asked ends; a newer lift is left riding.
    pub fn landed(&mut self, serial: u64) -> bool {
        let ours = self.riding().is_some_and(|lifted| lifted.serial == serial);
        if ours {
            self.phase = Phase::Idle;
        }
        ours
    }
}
