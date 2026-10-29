//! The bounded row model shared by every view; transitions publish one complete snapshot.
use crate::pane::{
    requests::{self, Requests, RowAction},
    states::{Kind, State},
    swap::{Options, Own, Swap},
};
use flea::protocol::{Reply, RowFacts};
use std::rc::Rc;

pub use super::element::ROW_HEIGHT;

#[derive(Default)]
pub struct Held {
    pub listing: usize,
    pub total: usize,
    pub start: usize,
    pub rows: Vec<RowFacts>,
}

impl Held {
    pub fn row(&self, index: usize) -> Option<&RowFacts> {
        index
            .checked_sub(self.start)
            .and_then(|at| self.rows.get(at))
    }
}

pub fn visible_rows(height: f32) -> usize {
    ((height / ROW_HEIGHT).ceil() as usize).max(1)
}

/// The band a refused rename draws under the row it edits: that row grows by the band and every row below it moves down by the band.
/// With no band every method is the fixed-row arithmetic the views always used.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Grow {
    /// The view position of the edited row.
    pub at: usize,
    pub band: f32,
}

impl Grow {
    /// The band for the row at a listing index, or none when no row is edited or the view filters it out.
    pub fn of_row(index: Option<usize>, band: f32, shown: Option<&[usize]>) -> Self {
        let at = match (index, shown) {
            (Some(index), Some(rows)) => rows.iter().position(|&row| row == index),
            (Some(index), None) => Some(index),
            (None, _) => None,
        };
        match at {
            Some(at) if band > 0. => Self { at, band },
            _ => Self::default(),
        }
    }

    /// The top of a view position, before scroll.
    pub fn top(self, position: usize, row: f32) -> f32 {
        let below = self.band > 0. && position > self.at;
        position as f32 * row + if below { self.band } else { 0. }
    }

    /// The height of a view position: the edited row takes the band.
    pub fn height(self, position: usize, row: f32) -> f32 {
        row + if self.band > 0. && position == self.at {
            self.band
        } else {
            0.
        }
    }

    /// The position drawn at a content offset, scroll already added; the band stays on its edited row.
    pub fn position_at(self, y: f32, row: f32) -> usize {
        let y = y.max(0.);
        if self.band <= 0. {
            return (y / row).floor() as usize;
        }
        let edited = self.at as f32 * row;
        if y < edited {
            (y / row).floor() as usize
        } else if y < edited + row + self.band {
            self.at
        } else {
            self.at + 1 + ((y - edited - row - self.band) / row).floor() as usize
        }
    }

    /// The exclusive end position of a content offset, rounded up the way the views always rounded.
    pub fn end_at(self, y: f32, row: f32) -> usize {
        if self.band <= 0. {
            (y.max(0.) / row).ceil() as usize
        } else {
            self.position_at(y, row) + 1
        }
    }

    /// The content height of `count` rows, the band included once the edited row is among them.
    pub fn content(self, count: usize, row: f32) -> f32 {
        count as f32 * row
            + if self.band > 0. && count > self.at {
                self.band
            } else {
                0.
            }
    }

    /// The first and end positions a viewport of `height` at `scroll` draws, the end capped at `count`.
    pub fn span(self, scroll: f32, height: f32, row: f32, count: usize) -> (usize, usize) {
        let first = self.position_at(scroll, row);
        let end = self.end_at(scroll + height, row).min(count);
        (first, end.max(first))
    }
}

/// Focus.js:168-169 pages floor(visibleRows / 2) items, at least one, in every view.
pub fn page_items(visible: usize) -> usize {
    (visible / 2).max(1)
}

pub fn window_size(visible: usize) -> usize {
    visible.saturating_add(2 * requests::BUFFER)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

pub struct Pane {
    path: String,
    pub held: Rc<Held>,
    pub scroll_y: f32,
    pub scroll_extent: Option<f32>,
    pub cursor: usize,
    height: f32,
    row_height: f32,
    pub state: State,
    pub swap: Swap,
    pub requests: Requests,
    pub list_requests: usize,
    /// Backend.qml firstRowsAt: wall-clock ms when this pane's backend first delivered rows, 0 before.
    pub first_rows_at: u64,
    pub(crate) greatest_listing: usize,
    pub(crate) clock: crate::pane::clock::Clock,
}

impl Pane {
    pub fn new(path: String, height: f32) -> Self {
        Self::at_row_height(path, height, ROW_HEIGHT)
    }

    pub fn at_row_height(path: String, height: f32, row_height: f32) -> Self {
        let mut pane = Self {
            path: path.clone(),
            held: Rc::default(),
            scroll_y: 0.,
            scroll_extent: None,
            cursor: 0,
            height,
            row_height,
            state: State::default(),
            swap: Swap::default(),
            requests: Requests::default(),
            list_requests: 0,
            first_rows_at: 0,
            greatest_listing: 0,
            clock: Default::default(),
        };
        pane.begin(path, Options::default(), 0);
        pane
    }

    pub fn now(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }
    pub(crate) fn use_executor(&mut self, executor: gpui::BackgroundExecutor) {
        self.clock.bind(executor);
    }
    pub(crate) fn identity(&self) -> u64 {
        self.clock.identity
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn place(&self) -> &str {
        self.swap
            .pending
            .as_ref()
            .map_or(self.path(), |pending| pending.path.as_str())
    }

    /// A listing asked for that has not landed yet, Pane.qml's listInFlight.
    pub fn listing_in_flight(&self) -> bool {
        self.busy()
    }

    /// Unanswered listing and held-window requests.
    pub fn pending(&self) -> usize {
        usize::from(self.busy()) + usize::from(self.requests.outstanding.is_some())
    }

    /// The listing request's first count, one window as Pane.qml asks for it.
    pub fn first_count(&self) -> usize {
        window_size(self.visible_rows())
    }
    pub fn visible_rows(&self) -> usize {
        ((self.height / self.row_height).ceil() as usize).max(1)
    }
    pub fn set_height(&mut self, height: f32) {
        self.height = height;
        self.scroll_by(0.);
    }
    pub fn busy(&self) -> bool {
        self.swap.busy()
    }

    pub fn begin(&mut self, path: String, options: Options, now: u64) -> bool {
        if self.busy() && path == self.place() {
            return false;
        }
        if !self
            .swap
            .begin(path, self.first_count(), &self.state, options, now)
        {
            return false;
        }
        self.list_requests += 1;
        self.requests.cancel();
        if !self.swap.holding {
            self.clear_rows();
        }
        true
    }

    fn clear_rows(&mut self) {
        self.scroll_extent = None;
        self.held = Rc::default();
        self.cursor = 0;
        self.scroll_y = 0.;
        self.state = State::default();
    }

    pub fn receive(&mut self, reply: Reply) -> Result<bool, String> {
        self.receive_at(reply, self.now())
    }

    pub fn receive_at(&mut self, reply: Reply, now: u64) -> Result<bool, String> {
        match reply {
            Reply::Listed { path, total } => {
                if let Some(pending) = &mut self.swap.pending {
                    if pending.ready.is_none() {
                        pending.total = (path.is_empty() || path == pending.path).then_some(total);
                    }
                } else {
                    self.swap.unheld_total =
                        (path.is_empty() || path == self.path).then_some(total);
                    // Each accepted sort answers one listed line, so its asked count becomes the pair's own here.
                    self.swap.unheld_own = match self.swap.sorts_out.pop_front() {
                        Some(first) => Own::Sort(first),
                        None => Own::Listing(self.swap.listing_first),
                    };
                }
                Ok(false)
            }
            Reply::Rows {
                start,
                rows,
                listing,
            } => {
                if self.first_rows_at == 0 && !rows.is_empty() {
                    self.first_rows_at = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(1, |since| since.as_millis().max(1) as u64);
                }
                self.take_rows(start, rows, listing, now)
            }
            Reply::Error {
                operation,
                path,
                message,
                mode,
            } => Ok(self.fail(&operation, &path, &message, mode, now)),
            _ => Ok(false),
        }
    }

    pub(crate) fn commit(&mut self, now: u64) -> bool {
        let Some(mut pending) = self.swap.pending.take() else {
            return false;
        };
        let Some(held) = pending.ready.take() else {
            self.swap.pending = Some(pending);
            return false;
        };
        let held = pending.following.take().unwrap_or(held);
        self.scroll_extent = None;
        self.path = pending.path;
        self.swap.listing_first = pending.first;
        self.greatest_listing = held.listing;
        self.cursor = held.start;
        self.scroll_y = held.start as f32 * self.row_height;
        self.state = State {
            kind: if held.total == 0 {
                Kind::Empty
            } else {
                Kind::Ready
            },
            target: self.path.clone(),
            query: pending.options.kept_query,
            search: pending.options.search,
            ..State::default()
        };
        self.held = Rc::new(held);
        self.scroll_by(0.);
        self.requests.initial_first = Some((self.scroll_y / self.row_height).floor() as usize);
        self.swap.counters.commits += 1;
        self.swap.rows_at = now;
        self.swap.end(now, "landed");
        true
    }

    pub fn tick(&mut self, now: u64) -> bool {
        if self.swap.pending.as_ref().is_some_and(|pending| {
            pending.ready.is_some() && pending.parked_until.is_none_or(|until| now >= until)
        }) && self.swap.can_land(now)
        {
            if self
                .swap
                .pending
                .as_ref()
                .is_some_and(|pending| pending.parked_until.is_some())
            {
                self.retire_anchor();
            }
            return self.commit(now);
        }
        if self.swap.expire(now) {
            self.clear_rows();
            return true;
        }
        false
    }

    pub fn frame(&mut self, now: u64, loading_mark_drawn: bool) {
        self.swap
            .mark_shown(loading_mark_drawn && self.state.kind == Kind::Loading, now);
        if self.busy() && self.state.kind == Kind::Loading {
            if self.swap.fell_back {
                self.swap.counters.loading_frames += 1;
            } else {
                self.swap.counters.blank_frames += 1;
            }
        }
    }

    pub fn row_action(&self, rows: Vec<usize>) -> Option<RowAction> {
        requests::capture(&self.held, self.swap.pending.is_some(), rows)
    }
    pub fn accepts_work(&self, listing: usize) -> bool {
        self.swap.pending.is_none() && listing > 0 && listing == self.held.listing
    }
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pane_grow_tests.rs"]
mod grow_tests;

#[path = "pane_geometry.rs"]
mod geometry;

#[path = "pane_failure.rs"]
mod failure;
