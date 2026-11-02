//! The figure service: a lazy helper answers by ticket, an overdue head fails alone and the queued tickets retry on a fresh helper.

use super::markdown_figure::{cache_key, request_line, Theme};
use super::markdown_figure_store::Store;
use super::markdown_model::FigureKind;
use std::collections::{HashMap, VecDeque};

#[path = "markdown_figure_events.rs"]
mod events;

/// The longest one figure may render before its helper is killed.
pub const RENDER_MS: u64 = 2000;
/// With nothing waiting this long the helper exits, so its memory returns.
pub const IDLE_EXIT_MS: u64 = 30_000;
/// Revisits and theme flips back answer from here, never rendered again.
pub const CACHE_MAX: usize = 64;
/// The launcher's refusal status, which latches the fenced-source fallback.
pub const REFUSAL_EXIT: i32 = 127;
/// An unexpected exit strikes the head ticket, and its second strike fails it.
pub const EXIT_STRIKE_LIMIT: u32 = 2;
/// What a refused or absent engine answers each figure.
pub const NOT_STARTED: &str = "figure engine did not start";

/// What the host does for the service: start, feed or end the helper and the store, and hand an answer to its figure.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Starts `flea --figure-helper`, with `--warm=` and these kinds when there are any.
    HelperSpawn(Vec<&'static str>),
    HelperWrite(String),
    HelperKill,
    /// An idle stop: the helper is ended and its exit then reported.
    HelperStop,
    /// Starts `flea --figure-store`.
    StoreSpawn,
    StoreWrite(String),
    StoreKill,
    /// Closes the store's input so it drains and exits.
    StoreCloseInput,
    /// A ticket's answer: the drawn picture, or an error and an empty picture.
    Done {
        ticket: u64,
        svg: String,
        error: String,
    },
    Log(String),
}

struct Waiting {
    kind: FigureKind,
    source: String,
    display: bool,
    theme: Theme,
    generation: u64,
    deadline: u64,
    strikes: u32,
    on_disk: bool,
}

/// The service's state; every event takes the clock in milliseconds and leaves commands to take.
pub struct Service {
    pub render_ms: u64,
    pub idle_exit_ms: u64,
    pub available: bool,
    pub worker_answers: usize,
    pub sends: usize,
    pub helper_exits: usize,
    pub deadline_expirations: usize,
    pub warm_kinds: Vec<&'static str>,
    pub warm_keys: Vec<String>,
    pub disk: Store,
    seq: u64,
    waiting: HashMap<u64, Waiting>,
    cache: HashMap<String, String>,
    order: VecDeque<String>,
    pending: VecDeque<u64>,
    written: VecDeque<u64>,
    running: bool,
    starting: bool,
    stopping: bool,
    generation: u64,
    warm_query: u64,
    idle_at: Option<u64>,
    commands: Vec<Command>,
}

impl Default for Service {
    fn default() -> Self {
        Service {
            render_ms: RENDER_MS,
            idle_exit_ms: IDLE_EXIT_MS,
            available: true,
            worker_answers: 0,
            sends: 0,
            helper_exits: 0,
            deadline_expirations: 0,
            warm_kinds: Vec::new(),
            warm_keys: Vec::new(),
            disk: Store::default(),
            seq: 0,
            waiting: HashMap::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            pending: VecDeque::new(),
            written: VecDeque::new(),
            running: false,
            starting: false,
            stopping: false,
            generation: 0,
            warm_query: 0,
            idle_at: None,
            commands: Vec::new(),
        }
    }
}

impl Service {
    /// The commands the last events left, in order.
    pub fn take_commands(&mut self) -> Vec<Command> {
        let mut out = std::mem::take(&mut self.commands);
        out.extend(self.disk.take_commands());
        out
    }

    pub fn helper_running(&self) -> bool {
        self.running
    }

    pub fn waiting_count(&self) -> usize {
        self.waiting.len()
    }

    /// The deadline poll runs only while a ticket waits, so an idle session wakes for nothing.
    pub fn deadline_running(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// The held answer for exactly this figure, refreshed as the most recent.
    pub fn cached(
        &mut self,
        kind: FigureKind,
        source: &str,
        theme: &Theme,
        display: bool,
    ) -> Option<String> {
        let key = cache_key(kind, source, theme, display);
        let hit = self.cache.get(&key).cloned()?;
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
        Some(hit)
    }

    fn store(&mut self, key: String, svg: String) {
        self.order.retain(|k| *k != key);
        self.cache.insert(key.clone(), svg);
        self.order.push_back(key);
        while self.order.len() > CACHE_MAX {
            if let Some(old) = self.order.pop_front() {
                self.cache.remove(&old);
            }
        }
    }

    /// The earliest moment a deadline, the idle exit or the store's clocks fall due.
    pub fn next_wake(&self) -> Option<u64> {
        let head = self
            .written
            .front()
            .and_then(|id| self.waiting.get(id))
            .map(|w| w.deadline)
            .filter(|d| *d > 0);
        [head, self.idle_at, self.disk.next_wake()]
            .into_iter()
            .flatten()
            .min()
    }

    /// Runs whatever fell due by now: an overdue head is killed alone, an idle helper and store are stopped.
    pub fn tick(&mut self, now: u64) {
        for event in self.disk.tick(now) {
            self.store_event(event, now);
        }
        if let Some(&head) = self.written.front() {
            if self
                .waiting
                .get(&head)
                .is_some_and(|w| w.deadline > 0 && now >= w.deadline)
            {
                self.deadline_expirations += 1;
                self.kill_helper(head, now);
            }
        }
        if self.waiting.is_empty() && self.idle_at.is_none() {
            self.arm_idle(now);
        }
        if self.idle_at.is_some_and(|at| now >= at) {
            self.idle_at = None;
            if !self.waiting.is_empty() {
                return;
            }
            self.disk.stop(now);
            // A store still active is asked again at the next idle, so it never runs on for the session.
            if self.disk.active() {
                self.idle_at = Some(now + self.idle_exit_ms);
            }
            if self.running {
                self.stopping = true;
                self.commands.push(Command::HelperStop);
            }
        }
    }

    fn arm_idle(&mut self, now: u64) {
        if self.waiting.is_empty() && (self.running || self.disk.active()) && !self.stopping {
            self.idle_at = Some(now + self.idle_exit_ms);
        }
    }

    /// Asks for one figure; the answer comes back as a Done for the returned ticket, a held one at once.
    pub fn ask(
        &mut self,
        kind: FigureKind,
        source: &str,
        display: bool,
        theme: &Theme,
        now: u64,
    ) -> u64 {
        self.seq += 1;
        let id = self.seq;
        if let Some(svg) = self.cached(kind, source, theme, display) {
            self.done(id, svg, String::new());
            return id;
        }
        if !self.available {
            self.done(id, String::new(), NOT_STARTED.to_string());
            return id;
        }
        let on_disk = self.disk.available;
        self.waiting.insert(
            id,
            Waiting {
                kind,
                source: source.to_string(),
                display,
                theme: theme.clone(),
                generation: 0,
                deadline: 0,
                strikes: 0,
                on_disk,
            },
        );
        self.idle_at = None;
        if on_disk {
            for event in self
                .disk
                .get(id, &cache_key(kind, source, theme, display), now)
            {
                self.store_event(event, now);
            }
        } else {
            self.dispatch(id, now);
        }
        id
    }

    fn done(&mut self, ticket: u64, svg: String, error: String) {
        self.commands.push(Command::Done { ticket, svg, error });
    }

    fn dispatch(&mut self, id: u64, now: u64) {
        if self.running && !self.starting && !self.stopping {
            self.write_line(id, now);
        } else {
            self.pending.push_back(id);
            self.ensure_helper();
        }
    }

    fn ensure_helper(&mut self) -> bool {
        if !self.available {
            return false;
        }
        if self.stopping || self.running || self.starting {
            return true;
        }
        self.generation += 1;
        self.starting = true;
        self.idle_at = None;
        // A queued figure initializes its own kind; throwaway warm renders would delay its answer.
        let warm = if self.pending.is_empty() {
            self.warm_kinds.clone()
        } else {
            Vec::new()
        };
        self.commands.push(Command::HelperSpawn(warm));
        true
    }

    fn write_line(&mut self, id: u64, now: u64) {
        let (generation, stopping, running) = (self.generation, self.stopping, self.running);
        let first = self.written.is_empty();
        let render_ms = self.render_ms;
        let Some(w) = self
            .waiting
            .get_mut(&id)
            .filter(|w| w.generation == 0 && !stopping && running)
        else {
            return;
        };
        w.generation = generation;
        if first {
            w.deadline = now + render_ms;
        }
        let line = request_line(id, w.kind, &w.source, w.display, &w.theme);
        self.written.push_back(id);
        self.sends += 1;
        self.commands.push(Command::HelperWrite(line));
    }
}

#[cfg(test)]
#[path = "markdown_figure_service_tests.rs"]
mod tests;
