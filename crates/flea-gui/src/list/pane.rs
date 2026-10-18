//! The bounded row model shared by every view; transitions publish one complete snapshot.
use crate::pane::{
    requests::{self, Requests, RowAction},
    states::{Kind, State},
    swap::{Options, Swap},
};
use flea::protocol::{Reply, RowFacts};
use std::{rc::Rc, time::Instant};

pub const ROW_HEIGHT: f32 = 31.;

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
    pub cursor: usize,
    height: f32,
    pub state: State,
    pub swap: Swap,
    pub requests: Requests,
    pub list_requests: usize,
    pub(crate) greatest_listing: usize,
    clock: Instant,
}

impl Pane {
    pub fn new(path: String, height: f32) -> Self {
        let mut pane = Self {
            path: path.clone(),
            held: Rc::default(),
            scroll_y: 0.,
            cursor: 0,
            height,
            state: State::default(),
            swap: Swap::default(),
            requests: Requests::default(),
            list_requests: 0,
            greatest_listing: 0,
            clock: Instant::now(),
        };
        pane.begin(path, Options::default(), 0);
        pane
    }

    pub fn now(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
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
    pub fn first_count(&self) -> usize {
        window_size(self.visible_rows())
    }
    pub fn visible_rows(&self) -> usize {
        visible_rows(self.height)
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
                }
                Ok(false)
            }
            Reply::Rows {
                start,
                rows,
                listing,
            } => self.take_rows(start, rows, listing, now),
            Reply::Error {
                operation,
                path,
                message,
            } => Ok(self.fail(&operation, &path, &message, 0, now)),
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
        self.path = pending.path;
        self.swap.unheld_first = pending.first;
        self.greatest_listing = held.listing;
        self.cursor = held.start;
        self.scroll_y = held.start as f32 * ROW_HEIGHT;
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
        self.requests.initial_first = Some((self.scroll_y / ROW_HEIGHT).floor() as usize);
        self.swap.counters.commits += 1;
        self.swap.rows_at = now;
        self.swap.end(now, "landed");
        true
    }

    pub fn tick(&mut self, now: u64) -> bool {
        if self.swap.pending.as_ref().is_some_and(|pending| {
            pending.ready.is_some() && pending.parked_until.is_none_or(|until| now >= until)
        }) && self.swap.show_left(now) == 0
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

    pub fn fail(
        &mut self,
        operation: &str,
        path: &str,
        message: &str,
        mode: u32,
        now: u64,
    ) -> bool {
        if operation == "stale" || operation == "sort" {
            return false;
        }
        if matches!(operation, "scan" | "window") && !path.is_empty() && path != self.place() {
            return false;
        }
        if operation != "backend"
            && self
                .swap
                .pending
                .as_ref()
                .is_some_and(|pending| pending.parked_until.is_some())
        {
            if operation == "window" {
                self.retire_anchor();
            }
            self.swap
                .pending
                .as_mut()
                .expect("parked listing")
                .parked_until = None;
            return self.swap.show_left(now) == 0 && self.commit(now);
        }
        if operation == "window" {
            if path == self.path && !self.busy() {
                self.requests.cancel();
            } else if path == self.place() {
                self.retire_anchor();
            }
            return false;
        }
        if (!self.busy() && operation != "backend")
            || (operation == "scan" && !path.is_empty() && path != self.place())
        {
            return false;
        }
        let target = self.place().to_owned();
        self.requests.cancel();
        self.clear_rows();
        self.state = State {
            kind: if operation == "scan" && message.to_lowercase().contains("permission denied") {
                Kind::Locked
            } else {
                Kind::Error
            },
            target,
            mode,
            ..State::default()
        };
        self.swap.end(now, "dropped");
        true
    }

    pub fn row_action(&self, rows: Vec<usize>) -> Option<RowAction> {
        requests::capture(&self.held, self.swap.pending.is_some(), rows)
    }
    pub fn accepts_work(&self, listing: usize) -> bool {
        self.swap.pending.is_none() && listing > 0 && listing == self.held.listing
    }
    pub fn wanted_window(&mut self) -> Option<(usize, usize)> {
        if self.swap.pending.is_some() {
            return None;
        }
        let first = (self.scroll_y / ROW_HEIGHT).floor() as usize;
        self.requests
            .wanted(&self.held, first, self.visible_rows())
            .map(|request| (request.start, request.count))
    }
    fn max_scroll(&self) -> f32 {
        (self.held.total as f32 * ROW_HEIGHT - self.height).max(0.)
    }
    pub fn scroll_by(&mut self, delta: f32) {
        self.scroll_y = (self.scroll_y + delta).clamp(0., self.max_scroll());
    }
    pub fn scroll_to(&mut self, y: f32) {
        self.scroll_y = y.clamp(0., self.max_scroll());
    }
    pub fn move_cursor(&mut self, step: Move) -> bool {
        let total = self.held.total;
        if self.busy() || total == 0 {
            return false;
        }
        let page = ((self.height / ROW_HEIGHT).floor() as usize).max(1);
        let last = total - 1;
        self.cursor = match step {
            Move::Up => self.cursor.saturating_sub(1),
            Move::Down => (self.cursor + 1).min(last),
            Move::PageUp => self.cursor.saturating_sub(page),
            Move::PageDown => (self.cursor + page).min(last),
            Move::Home => 0,
            Move::End => last,
        };
        self.reveal_cursor();
        true
    }
    pub fn reveal_cursor(&mut self) {
        let top = self.cursor as f32 * ROW_HEIGHT;
        if top < self.scroll_y {
            self.scroll_to(top);
        } else if top + ROW_HEIGHT > self.scroll_y + self.height {
            self.scroll_to(top + ROW_HEIGHT - self.height);
        }
    }
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
