//! The listing's cap and minimum drawn time, with a complete pair retained until one commit.
use super::{states::State, Held};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::Duration,
};

mod progress;
pub use progress::{Clock, Progress};

pub const CAP_MS: u64 = 66;
pub const HOLD_MS: u64 = 150;
pub const MIN_SHOW_MS: u64 = 100;
pub const PARKED_WAIT_MS: u64 = crate::window::startup::LISTING_WAIT_MS;

pub fn show_left(since: Option<u64>, now: u64) -> u64 {
    since.map_or(0, |since| {
        since.saturating_add(MIN_SHOW_MS).saturating_sub(now)
    })
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// A settled folder is updated in place and never gives way to a loading frame.
    pub in_place: bool,
    /// Grid owns its viewport and already holds its presented tiles until their replacement is ready.
    pub independent_viewport: bool,
    pub supersede: bool,
    pub clear_at_once: bool,
    pub kept_query: String,
    pub search: bool,
    pub hold_walk: bool,
    pub restore_name: Option<String>,
    // The Esc relist a search exit arms; only its failure drops the cursor file armed for the next listing.
    pub relist: bool,
}

pub struct Pending {
    pub path: String,
    pub total: Option<usize>,
    pub first: usize,
    pub ready: Option<Held>,
    pub following: Option<Held>,
    pub parked_until: Option<u64>,
    pub retained_start: Option<usize>,
    pub retained_cursor: Option<usize>,
    pub options: Options,
}

#[derive(Default)]
pub struct Session {
    pub open: bool,
    pub leaving: bool,
    pub generation: usize,
    pub opened_generation: usize,
    pub query: String,
}

impl Session {
    pub fn drops(&self, generation: usize) -> bool {
        generation > 0
            && (self.leaving
                || !self.open
                || generation != self.generation
                || generation <= self.opened_generation)
    }
}

#[derive(Default, Debug)]
pub struct Counters {
    pub holds: usize,
    pub fallbacks: usize,
    pub waits: usize,
    pub blank_frames: usize,
    pub loading_frames: usize,
    pub commits: usize,
    pub shown_ms: u64,
    pub last_ms: u64,
    pub last_end: &'static str,
}

#[derive(Default)]
pub struct Frames {
    pub blank: Cell<usize>,
    pub loading: Cell<usize>,
}

impl Frames {
    pub fn record(&self, busy: bool, loading: bool, fell_back: bool) {
        if busy && loading {
            let counter = if fell_back {
                &self.loading
            } else {
                &self.blank
            };
            counter.set(counter.get() + 1);
        }
    }
}

// What answers an armed pair, set each time a listed line arms it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Own {
    // A sort's own window at this count, which a window error can answer.
    Sort(usize),
    // The listing's own first rows at this count, which only a rows line answers.
    Listing(usize),
    // Its own window failed or was refused, so only the window asked at an answer's start lands it.
    #[default]
    Asked,
}

#[derive(Default)]
pub struct Swap {
    /// The previous wire numbering retained by the last in-place commit.
    pub(crate) retained_listing: Option<usize>,
    pub pending: Option<Pending>,
    pub(crate) unheld_total: Option<usize>,
    pub(crate) unheld_own: Own,
    // The first-window count the committed listing asked for.
    pub(crate) listing_first: usize,
    // The search generation and first-window count its query sent, including walking reanswers.
    pub(crate) search_first: Option<(usize, usize)>,
    // The first-window count each sort still out asked for, oldest first.
    pub(crate) sorts_out: VecDeque<usize>,
    pub session: Session,
    pub holding: bool,
    pub fell_back: bool,
    pub since: u64,
    pub mark_since: Option<u64>,
    pub(crate) observed: Rc<RefCell<super::presentation::Observed>>,
    pub counters: Counters,
    pub frames: Rc<Frames>,
    pub mark: Rc<crate::feedback::spinner::Crawl>,
    pub input_at: u64,
    pub rows_at: u64,
}

impl Swap {
    pub(crate) fn publish_search(&mut self, session: Session, first: usize) {
        if session.generation != self.session.generation {
            self.search_first = Some((session.generation, first));
        }
        self.session = session;
    }

    /// inputToRows' second half: stamped once, and only after an input, as v0.3.12's PaneSwap.qml does; the launch listing is no input.
    pub(crate) fn stamp_rows(&mut self, now: u64) {
        if self.rows_at == 0 && self.input_at > 0 {
            self.rows_at = now;
        }
    }

    pub fn begin(
        &mut self,
        path: String,
        first: usize,
        state: &State,
        options: Options,
        now: u64,
    ) -> bool {
        if self.busy() && !options.supersede {
            return false;
        }
        self.mark_shown(false, now);
        let available = self.observed.borrow().available;
        self.observed = Rc::new(RefCell::new(super::presentation::Observed {
            available,
            ..Default::default()
        }));
        self.holding = !options.clear_at_once
            && state.kind != super::states::Kind::Loading
            && (!state.search || options.hold_walk);
        if self.holding {
            self.counters.holds += 1;
        }
        self.fell_back = false;
        self.since = now;
        self.input_at = now;
        self.rows_at = 0;
        self.unheld_total = None;
        self.sorts_out.clear();
        self.pending = Some(Pending {
            path,
            total: None,
            first,
            ready: None,
            following: None,
            parked_until: None,
            retained_start: None,
            retained_cursor: None,
            options,
        });
        true
    }

    pub fn busy(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| !pending.options.search)
    }

    pub(crate) fn retains_rows(&self) -> bool {
        self.holding
            && self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.options.in_place)
    }

    pub fn expire(&mut self, now: u64) -> bool {
        if !self.busy()
            || self.retains_rows()
            || self.fell_back
            || now < self.since.saturating_add(CAP_MS)
        {
            return false;
        }
        if self.holding {
            self.counters.fallbacks += 1;
        }
        self.holding = false;
        self.fell_back = true;
        self.counters.last_ms = now.saturating_sub(self.since);
        self.counters.last_end = "expired";
        true
    }

    pub fn show_left(&self, now: u64) -> u64 {
        show_left(self.mark_since, now)
    }

    pub fn awaiting_visibility(&self) -> bool {
        let observed = self.observed.borrow();
        self.mark_since.is_none() && !observed.wait_unavailable && observed.outstanding_loading > 0
    }

    pub fn can_land(&self, now: u64) -> bool {
        !self.awaiting_visibility() && self.show_left(now) == 0
    }

    pub fn mark_shown(&mut self, shown: bool, now: u64) {
        match (shown, self.mark_since) {
            (true, None) => self.mark_since = Some(now),
            (false, Some(since)) => {
                self.counters.shown_ms = now.saturating_sub(since);
                self.mark_since = None;
            }
            _ => {}
        }
    }

    // The crawl starts with the first frame that draws the mark; reduced motion keeps it at rest.
    pub fn crawling(&self, reduced_motion: bool) -> bool {
        !reduced_motion && self.mark.shown()
    }

    pub fn deadline(&self, now: u64) -> Option<Duration> {
        if !self.busy() {
            return None;
        }
        let pending = self.pending.as_ref()?;
        if pending.retained_start.is_some() {
            return None;
        }
        if self.retains_rows() && pending.ready.is_none() {
            return None;
        }
        if pending.ready.is_some() && self.awaiting_visibility() {
            return None; // The window's existing transition timer owns the feedback deadline.
        }
        if pending.ready.is_some() && pending.parked_until.is_none() {
            return Some(Duration::from_millis(self.show_left(now)));
        }
        let at = if self.fell_back {
            pending.parked_until?
        } else {
            self.since.saturating_add(CAP_MS)
        };
        Some(Duration::from_millis(at.saturating_sub(now)))
    }

    pub fn end(&mut self, now: u64, reason: &'static str) {
        if self.mark_since.is_none() && self.observed.borrow().loading_submissions > 0 {
            self.observed.borrow_mut().unobserved_releases += 1;
        }
        self.pending = None;
        self.unheld_total = None;
        self.sorts_out.clear();
        // The trace measures the hold, which already ended when its cap expired.
        if !self.fell_back {
            self.counters.last_ms = now.saturating_sub(self.since);
            self.counters.last_end = reason;
        }
        self.holding = false;
        self.fell_back = false;
        self.mark_shown(false, now);
    }
}

impl super::Pane {
    // A live search re-answer keeps the cursor's file, as PaneSwap.qml does; any other landing starts at the window.
    pub(crate) fn landing_cursor(
        &self,
        pending: &Pending,
        held: &crate::list::pane::Held,
    ) -> usize {
        if pending.options.in_place {
            return pending
                .options
                .restore_name
                .as_ref()
                .and_then(|name| held.rows.iter().position(|row| &row.name == name))
                .map(|at| held.start + at)
                .unwrap_or(self.cursor)
                .min(held.total.saturating_sub(1));
        }
        let reanswer = pending.options.search
            && self.state.search
            && self.path() == pending.path
            && self.state.query == pending.options.kept_query;
        let kept = reanswer.then(|| self.held.row(self.cursor)).flatten();
        flea::ui_logic_file_search::Session::kept_index(
            kept.map(|row| row.name.as_str()),
            held.rows.iter().map(|row| row.name.as_str()),
            held.start,
        )
    }

    pub fn receive_session(
        &mut self,
        reply: flea::protocol::Reply,
        generation: usize,
        now: u64,
    ) -> Result<bool, String> {
        if self.swap.session.drops(generation) {
            return Ok(false);
        }
        if generation == 0 {
            return self.receive_at(reply, now);
        }
        if let flea::protocol::Reply::Listed { path, total } = &reply {
            if self.busy() {
                return Ok(false);
            }
            self.swap.unheld_total = None;
            self.swap.pending = Some(Pending {
                path: path.clone(),
                total: Some(*total),
                first: self
                    .swap
                    .search_first
                    .filter(|(sent, _)| *sent == generation)
                    .map_or_else(|| self.first_count(), |(_, first)| first),
                ready: None,
                following: None,
                parked_until: None,
                retained_start: None,
                retained_cursor: None,
                options: Options {
                    search: true,
                    kept_query: self.swap.session.query.clone(),
                    ..Options::default()
                },
            });
            return Ok(false);
        }
        self.receive_at(reply, now)
    }
}
