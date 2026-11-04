//! ColumnsLanding.qml: a changing ancestor keeps its slot's last rows until its reply or the cap, the loading tile held its minimum.
use super::carry::Plan;
use super::peeks::{Lens, PeekRow, Peeks};
use crate::pane::swap::{CAP_MS, MIN_SHOW_MS};
use std::{
    collections::{hash_map::Entry, HashMap},
    rc::Rc,
};

/// One incoming ancestor slot's stand-in: the rows the slot drew before, until its own reply or the cap.
#[derive(Clone, Debug, PartialEq)]
pub struct Stand {
    pub rows: Rc<[PeekRow]>,
    pub mode: Option<u32>,
    pub ready: bool,
    pub loading: bool,
    pub shown_at: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct State {
    target: String,
    held: HashMap<String, Stand>,
    expired: bool,
    minimum: bool,
    started_at: u64,
    mark_since: u64,
}

impl State {
    fn loading(&self, path: &str) -> bool {
        self.expired || self.held.get(path).is_some_and(|stand| stand.loading)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Current,
    Previous,
}

/// The landing in flight and the one a carried superseding request inherited its cap and minimum from.
#[derive(Clone, Debug, Default)]
pub struct Landing {
    current: State,
    previous: Option<State>,
    pub version: u64,
}

impl Landing {
    fn state(&self, which: Which) -> Option<&State> {
        match which {
            Which::Current => Some(&self.current),
            Which::Previous => self.previous.as_ref(),
        }
    }

    fn state_mut(&mut self, which: Which) -> Option<&mut State> {
        match which {
            Which::Current => Some(&mut self.current),
            Which::Previous => self.previous.as_mut(),
        }
    }

    fn which_for(&self, path: &str, here: &str) -> Option<Which> {
        let which = if here == self.current.target {
            Which::Current
        } else {
            Which::Previous
        };
        self.state(which)
            .filter(|state| state.target == here && state.held.contains_key(path))
            .map(|_| which)
    }

    /// begin(): asks are the new listing's ancestors, old the current path's; each unanswered incoming slot inherits the rows its slot drew.
    pub fn begin(
        &mut self,
        carry: Option<&Plan>,
        asks: &[String],
        old: &[String],
        peeks: &Peeks,
        lens: &Lens<'_>,
        now: u64,
    ) {
        let here = lens.here;
        let mut next = HashMap::new();
        let mut inherited = 0;
        if let Some(carry) = carry {
            for (path, from) in asks
                .iter()
                .rev()
                .zip(old.iter().rev().map(Some).chain(std::iter::repeat(None)))
            {
                let seeded = *path == carry.seed && carry.rows.is_some();
                let Some(from) = from.filter(|_| !lens.ready(peeks, path) && !seeded) else {
                    continue;
                };
                let which = self.which_for(from, here);
                let reading = which.is_some_and(|which| {
                    self.state(which).is_some_and(|state| state.loading(from))
                });
                let since = which
                    .filter(|_| reading)
                    .and_then(|which| self.state(which)?.held.get(from.as_str())?.shown_at);
                next.insert(
                    path.clone(),
                    Stand {
                        rows: self.rows(
                            from,
                            here,
                            peeks
                                .rows(&lens.key(from))
                                .cloned()
                                .unwrap_or_else(|| Rc::from([])),
                        ),
                        mode: self.mode(from, here, peeks.denied(&lens.key(from))),
                        ready: false,
                        loading: reading,
                        shown_at: since,
                    },
                );
                inherited = inherited.max(since.unwrap_or(0));
            }
        }
        // Only a carried superseding request keeps the previous landing's cap and shown minimum.
        if carry.is_none() {
            self.previous = None;
        } else if self.current.target == here {
            self.previous = (!self.current.held.is_empty()).then(|| self.current.clone());
        }
        self.current = State {
            target: carry.map(|carry| carry.target.clone()).unwrap_or_default(),
            held: next,
            expired: false,
            minimum: inherited > 0 && inherited + MIN_SHOW_MS > now,
            started_at: now,
            mark_since: inherited,
        };
        self.version += 1;
    }

    /// The Columns view shown with replies still out: each such column stands in empty until its reply or the cap.
    pub fn enter(&mut self, here: &str, out: &[String], now: u64) {
        if out.is_empty() {
            return;
        }
        let stand = || Stand {
            rows: Rc::from([]),
            mode: None,
            ready: false,
            loading: false,
            shown_at: None,
        };
        self.previous = None;
        self.current = State {
            target: here.to_owned(),
            held: out.iter().map(|path| (path.clone(), stand())).collect(),
            started_at: now,
            ..State::default()
        };
        self.version += 1;
    }

    /// The held List remains interactive: replace its outstanding paths without restarting the entry cap.
    pub fn retarget_entry(&mut self, here: &str, out: &[String]) {
        if self.current.target != here {
            return;
        }
        let held = &mut self.current.held;
        let before = held.len();
        held.retain(|path, _| out.contains(path));
        let mut changed = held.len() != before;
        for path in out {
            if let Entry::Vacant(entry) = held.entry(path.clone()) {
                entry.insert(Stand {
                    rows: Rc::from([]),
                    mode: None,
                    ready: false,
                    loading: false,
                    shown_at: None,
                });
                changed = true;
            }
        }
        if changed {
            self.version += 1;
        }
    }

    /// A stand-in is still owed, answered or past its cap.
    pub fn standing(&self) -> bool {
        !self.current.held.is_empty()
    }

    /// True while a slot draws a stand-in, which the column takes no input on.
    pub fn waiting(&self, path: &str, here: &str) -> bool {
        self.which_for(path, here).is_some()
    }

    pub fn loading(&self, path: &str, here: &str) -> bool {
        self.which_for(path, here)
            .and_then(|which| self.state(which))
            .is_some_and(|state| state.loading(path))
    }

    /// The rows a slot draws: its stand-in, nothing once the stand-in reads as loading, else the ordinary rows.
    pub fn rows(&self, path: &str, here: &str, ordinary: Rc<[PeekRow]>) -> Rc<[PeekRow]> {
        match self
            .which_for(path, here)
            .and_then(|which| self.state(which))
        {
            None => ordinary,
            Some(state) if state.loading(path) => Rc::from([]),
            Some(state) => Rc::clone(&state.held[path].rows),
        }
    }

    pub fn mode(&self, path: &str, here: &str, ordinary: Option<u32>) -> Option<u32> {
        match self
            .which_for(path, here)
            .and_then(|which| self.state(which))
        {
            None => ordinary,
            Some(state) if state.loading(path) => None,
            Some(state) => state.held[path].mode,
        }
    }

    fn remove(&mut self, path: &str, which: Which) {
        if let Some(state) = self.state_mut(which) {
            state.held.remove(path);
            if state.held.is_empty() {
                state.minimum = false;
                if which == Which::Previous {
                    self.previous = None;
                }
            }
        }
        self.version += 1;
    }

    /// A slot's reply landed: a loading mark still inside its minimum turns ready, anything else drops the stand-in.
    pub fn answered(&mut self, path: &str) {
        for which in [Which::Current, Which::Previous] {
            let Some(state) = self.state_mut(which) else {
                continue;
            };
            if !state.held.contains_key(path) {
                continue;
            }
            if state.loading(path) && state.minimum {
                if let Some(stand) = state.held.get_mut(path) {
                    stand.ready = true;
                }
                self.version += 1;
            } else {
                self.remove(path, which);
            }
        }
    }

    /// The listing landed at the target, so the superseded landing ends.
    pub fn landed(&mut self, here: &str) {
        if here != self.current.target {
            return;
        }
        self.previous = None;
        self.version += 1;
    }

    /// Only the loading mark's shown signal starts the minimum.
    pub fn mark_shown(&mut self, path: &str, shown: bool, here: &str, now: u64) {
        let Some(which) = self.which_for(path, here) else {
            return;
        };
        let Some(state) = self.state_mut(which) else {
            return;
        };
        if !shown || !state.loading(path) {
            return;
        }
        if state
            .held
            .get(path)
            .is_some_and(|stand| stand.shown_at.is_some())
        {
            return;
        }
        if let Some(stand) = state.held.get_mut(path) {
            stand.shown_at = Some(now);
        }
        state.mark_since = state.mark_since.max(now);
        state.minimum = true;
        self.version += 1;
    }

    fn states(&self) -> impl Iterator<Item = &State> {
        std::iter::once(&self.current).chain(self.previous.as_ref())
    }

    /// The earliest cap still owed, shared by both landings without restarting either deadline.
    pub fn cap_deadline(&self) -> Option<u64> {
        self.states()
            .filter(|state| !state.expired && !state.held.is_empty())
            .map(|state| state.started_at + CAP_MS)
            .min()
    }

    pub fn minimum_deadline(&self) -> Option<u64> {
        self.states()
            .filter(|state| state.minimum)
            .map(|state| state.mark_since + MIN_SHOW_MS)
            .min()
    }

    /// The cap: every stand-in past it reads as loading.
    pub fn expire(&mut self, now: u64) {
        for which in [Which::Current, Which::Previous] {
            if let Some(state) = self
                .state_mut(which)
                .filter(|state| !state.expired && state.started_at + CAP_MS <= now)
            {
                state.expired = true;
            }
        }
        self.version += 1;
    }

    /// The minimum: a mark shown its full time releases, and every slot whose reply came in meanwhile lands.
    pub fn release_minimum(&mut self, now: u64) {
        for which in [Which::Current, Which::Previous] {
            let Some(state) = self.state_mut(which) else {
                continue;
            };
            if !state.minimum || state.mark_since + MIN_SHOW_MS > now {
                continue;
            }
            state.minimum = false;
            let ready: Vec<String> = state
                .held
                .iter()
                .filter(|(_, stand)| stand.ready)
                .map(|(path, _)| path.clone())
                .collect();
            self.version += 1;
            for path in ready {
                self.remove(&path, which);
            }
        }
    }

    /// One timer serves both deadlines: whichever came due runs.
    pub fn tick(&mut self, now: u64) {
        if self.cap_deadline().is_some_and(|at| at <= now) {
            self.expire(now);
        }
        if self.minimum_deadline().is_some_and(|at| at <= now) {
            self.release_minimum(now);
        }
    }

    pub fn next_deadline(&self) -> Option<u64> {
        self.cap_deadline()
            .into_iter()
            .chain(self.minimum_deadline())
            .min()
    }
}
