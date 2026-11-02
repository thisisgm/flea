//! A single bounded viewport request and the generation every row action must retain.
use super::{swap::Own, Held, Pane};
use flea::protocol::RowFacts;
use std::rc::Rc;

pub const BUFFER: usize = 150;
pub const MARGIN: usize = 25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRequest {
    pub start: usize,
    pub count: usize,
    pub listing: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowAction {
    pub listing: usize,
    pub rows: Vec<usize>,
}

#[derive(Default)]
pub struct Requests {
    pub outstanding: Option<WindowRequest>,
    pub refused: Option<WindowRequest>,
    pub sent: usize,
    pub initial_first: Option<usize>,
}

impl Requests {
    pub fn wanted(&mut self, held: &Held, first: usize, visible: usize) -> Option<WindowRequest> {
        if self.outstanding.is_some() || held.total == 0 {
            return None;
        }
        let last = first.saturating_add(visible).min(held.total);
        let end = held.start.saturating_add(held.rows.len()).min(held.total);
        if self.initial_first == Some(first) && first >= held.start && last <= end {
            return None;
        }
        self.initial_first = None;
        let drifted = held.rows.is_empty()
            || first < held.start
            || (first.saturating_sub(held.start) < MARGIN && held.start > 0)
            || (end.saturating_sub(last) < MARGIN && end < held.total);
        if !drifted {
            return None;
        }
        let request = WindowRequest {
            start: first.saturating_sub(BUFFER),
            count: visible.saturating_add(2 * BUFFER),
            listing: held.listing,
        };
        // A window answered malformed is asked again only after the listing or the wanted window changes.
        if self.refused.take() == Some(request) {
            self.refused = Some(request);
            return None;
        }
        self.outstanding = Some(request);
        self.sent += 1;
        Some(request)
    }

    pub fn answers(&self, held: &Held, start: usize, listing: usize) -> bool {
        self.outstanding.is_some_and(|request| {
            request.listing == listing && held.listing == listing && request.start == start
        })
    }

    pub fn accepts(&self, held: &Held, start: usize, count: usize, listing: usize) -> bool {
        self.answers(held, start, listing)
            && self
                .outstanding
                .is_some_and(|request| count <= request.count)
    }

    pub fn cancel(&mut self) {
        self.outstanding = None;
    }
}

pub fn capture(held: &Held, busy: bool, rows: Vec<usize>) -> Option<RowAction> {
    if busy || held.listing == 0 || rows.is_empty() || rows.iter().any(|&row| row >= held.total) {
        return None;
    }
    Some(RowAction {
        listing: held.listing,
        rows,
    })
}

pub fn current(action: &RowAction, held: &Held, busy: bool) -> bool {
    !busy && action.listing > 0 && action.listing == held.listing
}

pub fn swallows(busy: bool, action: &str) -> bool {
    busy && !action.is_empty()
        && !matches!(
            action,
            "open"
                | "parent"
                | "historyBack"
                | "historyForward"
                | "escape"
                | "viewList"
                | "viewColumns"
                | "viewGrid"
                | "viewDual"
                | "sidebar"
                | "windowNew"
                | "togglePreview"
                | "reload"
        )
}

impl Pane {
    /// A completed metadata write re-reads the held window through the same acceptance contract as scrolling.
    pub(crate) fn reread_held(&mut self) -> Option<WindowRequest> {
        if self.busy() || self.requests.outstanding.is_some() || self.held.total == 0 {
            return None;
        }
        let request = WindowRequest {
            start: self.held.start,
            count: self.first_count(),
            listing: self.held.listing,
        };
        self.requests.outstanding = Some(request);
        self.requests.sent += 1;
        Some(request)
    }

    pub fn anchor_window(&mut self, start: usize) -> Option<WindowRequest> {
        let (held, count) = match self.swap.pending.as_ref() {
            Some(pending) if pending.following.is_none() => {
                (pending.ready.as_ref()?, pending.first)
            }
            Some(_) => return None,
            None => (&*self.held, self.first_count()),
        };
        if self.requests.outstanding.is_some() || start >= held.total {
            return None;
        }
        let request = WindowRequest {
            start,
            count,
            listing: held.listing,
        };
        self.requests.outstanding = Some(request);
        self.requests.sent += 1;
        Some(request)
    }

    pub(crate) fn retire_anchor(&mut self) {
        // Pending listings admit only explicit anchor requests for their ready generation.
        if self.swap.pending.as_ref().is_some_and(|pending| {
            pending.ready.as_ref().is_some_and(|held| {
                self.requests
                    .outstanding
                    .is_some_and(|request| request.listing == held.listing)
            })
        }) {
            self.requests.cancel();
        }
    }

    pub(crate) fn take_rows(
        &mut self,
        start: usize,
        rows: Vec<RowFacts>,
        listing: usize,
        now: u64,
    ) -> Result<bool, String> {
        let answered = self
            .requests
            .outstanding
            .filter(|request| request.start == start && request.listing == listing);
        let taken = self.land_rows(start, rows, listing, now);
        // A malformed answer still settles the request it answers, so the slot never stays taken.
        if taken.is_err() && answered.is_some() && self.requests.outstanding == answered {
            self.requests.cancel();
            self.requests.refused = answered;
        }
        taken
    }

    fn land_rows(
        &mut self,
        start: usize,
        rows: Vec<RowFacts>,
        listing: usize,
        now: u64,
    ) -> Result<bool, String> {
        if self.swap.pending.is_some() {
            let pending = self.swap.pending.as_ref().expect("pending listing");
            let Some(total) = pending.total else {
                return Ok(false);
            };
            if let Some(held) = &pending.ready {
                // An answer to the anchor that breaks its bound is a protocol error, never a silent drop.
                if !self.requests.answers(held, start, listing) {
                    return Ok(false);
                }
                Self::check_complete(start, rows.len(), total, pending.first)?;
                let pending = self.swap.pending.as_mut().expect("pending listing");
                pending.following = Some(Held {
                    listing,
                    total,
                    start,
                    rows,
                });
                pending.parked_until = None;
                pending.retained_start = None;
                self.requests.cancel();
                self.swap.counters.waits += 1;
                return Ok(false);
            }
            if listing < self.greatest_listing
                || (listing == self.greatest_listing && !pending.options.search)
            {
                return Ok(false);
            }
            Self::check_complete(start, rows.len(), total, pending.first)?;
            let held = Held {
                listing,
                total,
                start,
                rows,
            };
            self.swap.pending.as_mut().expect("pending listing").ready = Some(held);
            if let Some((start, cursor)) = self.retained_window() {
                let pending = self.swap.pending.as_mut().expect("pending listing");
                pending.retained_start = Some(start);
                pending.retained_cursor = Some(cursor);
                return Ok(false);
            }
            let parked = self.swap.pending.as_ref().is_some_and(|pending| {
                !pending.options.in_place
                    && pending.options.restore_name.as_ref().is_some_and(|name| {
                        pending.ready.as_ref().is_some_and(|held| {
                            held.total > 0 && !held.rows.iter().any(|row| &row.name == name)
                        })
                    })
            });
            if parked {
                let until = now
                    .saturating_add(self.swap.show_left(now))
                    .saturating_add(super::swap::PARKED_WAIT_MS);
                self.swap
                    .pending
                    .as_mut()
                    .expect("pending listing")
                    .parked_until = Some(until);
                return Ok(false);
            }
            if !self
                .swap
                .pending
                .as_ref()
                .expect("pending listing")
                .options
                .search
                && !self.swap.can_land(now)
            {
                self.swap.counters.waits += 1;
                return Ok(false);
            }
            return Ok(self.commit(now));
        }
        // A same-path sort can end the hold before the relist's own pair (ui/PaneSwap.qml).
        if let Some(total) = self
            .swap
            .unheld_total
            .take()
            .filter(|_| listing > self.greatest_listing)
        {
            let own = self.swap.unheld_own;
            // With its own window failed, the pair lands only as the answer to the window asked at this start.
            let asked = match own {
                Own::Sort(first) | Own::Listing(first) => Some(first),
                Own::Asked => self
                    .requests
                    .outstanding
                    .filter(|request| request.start == start)
                    .map(|request| request.count),
            };
            let Some(bound) = asked else {
                self.swap.unheld_total = Some(total);
                return Ok(false);
            };
            if let Err(message) = Self::check_complete(start, rows.len(), total, bound) {
                // Refused rows never land, and the newer listing stays armed for the next window asked (ui/PaneSwap.qml takeRows).
                self.swap.unheld_total = Some(total);
                self.swap.unheld_own = Own::Asked;
                if own == Own::Asked {
                    self.requests.refused = self.requests.outstanding.take();
                }
                return Err(message);
            }
            self.requests.cancel();
            self.greatest_listing = listing;
            self.held = Rc::new(Held {
                listing,
                total,
                start,
                rows,
            });
            self.state.kind = if total == 0 {
                super::Kind::Empty
            } else {
                super::Kind::Ready
            };
            // Its own window lands the view at its first row (ui/PaneSwap.qml land); a scroll's answer leaves it (takeRows).
            if own != Own::Asked {
                self.cursor = start;
                self.scroll_y = start as f32 * self.row_height();
            }
            self.scroll_by(0.);
            self.requests.initial_first =
                Some((self.scroll_y / self.row_height()).floor() as usize);
            self.swap.counters.commits += 1;
            self.swap.stamp_rows(now);
            return Ok(true);
        }
        if listing == self.held.listing {
            let bound = self
                .requests
                .outstanding
                .map_or(self.first_count(), |request| request.count);
            Self::check_window(start, rows.len(), self.held.total, bound)?;
        }
        if !self
            .requests
            .accepts(&self.held, start, rows.len(), listing)
        {
            return Ok(false);
        }
        Self::check_complete(
            start,
            rows.len(),
            self.held.total,
            self.requests.outstanding.expect("matched window").count,
        )?;
        self.requests.cancel();
        self.held = Rc::new(Held {
            listing,
            total: self.held.total,
            start,
            rows,
        });
        Ok(true)
    }

    pub(crate) fn check_window(
        start: usize,
        count: usize,
        total: usize,
        bound: usize,
    ) -> Result<(), String> {
        if start > total
            || count > total.saturating_sub(start)
            || count > bound
            || (total > 0 && count == 0)
            || (total == 0 && start != 0)
        {
            return Err("pane.reply_window".into());
        }
        Ok(())
    }

    fn check_complete(
        start: usize,
        count: usize,
        total: usize,
        bound: usize,
    ) -> Result<(), String> {
        Self::check_window(start, count, total, bound)?;
        if count != total.saturating_sub(start).min(bound) {
            return Err("pane.reply_window".into());
        }
        Ok(())
    }
}
