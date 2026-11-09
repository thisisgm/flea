//! The service's process events: the helper and the store starting, answering, failing and exiting.

use super::super::markdown_figure::{cache_key, Theme};
use super::super::markdown_figure_store::StoreEvent;
use super::super::markdown_model::FigureKind;
use super::{Command, Service, EXIT_STRIKE_LIMIT, NOT_STARTED, REFUSAL_EXIT};

impl Service {
    /// The helper process is up: queued tickets are written in order.
    pub fn helper_started(&mut self, now: u64) {
        self.starting = false;
        self.running = true;
        if self.stopping {
            self.commands.push(Command::HelperKill);
            return;
        }
        self.warm_kinds.clear();
        for id in std::mem::take(&mut self.pending) {
            self.write_line(id, now);
        }
        // A warm start with nothing asked yet still ends at the idle exit.
        self.arm_idle(now);
    }

    /// The helper could not be started at all, which latches the fenced-source fallback.
    pub fn helper_spawn_failed(&mut self, now: u64) {
        if self.starting && !self.stopping {
            self.starting = false;
            self.refuse(NOT_STARTED, now);
        }
    }

    /// The helper exited: a refusal latches, a crash strikes its head ticket and requeues the rest on a fresh helper.
    pub fn helper_exited(&mut self, code: i32, now: u64) {
        self.helper_exits += 1;
        self.running = false;
        self.starting = false;
        self.stopping = true;
        let error = format!(
            "figure engine exited {code} (status {})",
            i32::from(code < 0)
        );
        if code == REFUSAL_EXIT {
            self.refuse(&error, now);
        } else {
            // A kill or an idle stop leaves nothing written, so only a crash finds a head to strike.
            let failed = self.strike_head();
            self.requeue_written();
            if let Some(failed) = failed {
                self.commands.push(Command::Log(
                    "FigureService: the figure engine stopped twice on one figure, which shows its fenced source".to_string(),
                ));
                self.done(failed, String::new(), error);
            }
        }
        self.stopping = false;
        if !self.pending.is_empty() {
            self.ensure_helper();
        }
    }

    fn strike_head(&mut self) -> Option<u64> {
        let head = *self.written.front()?;
        let w = self.waiting.get_mut(&head)?;
        w.strikes += 1;
        if w.strikes < EXIT_STRIKE_LIMIT {
            return None;
        }
        self.written.pop_front();
        self.waiting.remove(&head);
        Some(head)
    }

    fn requeue_written(&mut self) {
        let retry = std::mem::take(&mut self.written);
        for id in &retry {
            if let Some(w) = self.waiting.get_mut(id) {
                w.generation = 0;
                w.deadline = 0;
            }
        }
        for id in retry.into_iter().rev() {
            self.pending.push_front(id);
        }
    }

    pub(super) fn kill_helper(&mut self, id: u64, _now: u64) {
        self.stopping = true;
        self.waiting.remove(&id);
        self.written.retain(|ticket| *ticket != id);
        self.requeue_written();
        self.done(id, String::new(), "render timed out".to_string());
        if self.running {
            self.commands.push(Command::HelperKill);
        }
    }

    // A refusal fails every ticket into its fenced source without retries.
    fn refuse(&mut self, error: &str, _now: u64) {
        if !self.available {
            return;
        }
        self.available = false;
        self.commands.push(Command::Log(format!(
            "FigureService: {error}, figures show their fenced source"
        )));
        let mut ids: Vec<u64> = self
            .waiting
            .iter()
            .filter(|(_, w)| !w.on_disk)
            .map(|(id, _)| *id)
            .collect();
        ids.sort_unstable();
        for id in ids {
            self.waiting.remove(&id);
            self.done(id, String::new(), error.to_string());
        }
        self.pending.clear();
        self.written.clear();
    }

    /// One line from the helper: {"id":3,"svg":"<svg ...>"} or {"id":3,"error":"formula over 4 KiB"}.
    pub fn helper_line(&mut self, line: &str, now: u64) {
        let Ok(message) = crate::jsondoc::parse(line) else {
            return;
        };
        let Some(id) = message
            .get("id")
            .and_then(|id| id.as_f64())
            .filter(|id| *id >= 0.0)
            .map(|id| id as u64)
        else {
            return;
        };
        let current = self
            .waiting
            .get(&id)
            .is_some_and(|w| w.generation == self.generation);
        if !current || self.stopping {
            return;
        }
        self.worker_answers += 1;
        let Some(asked) = self.waiting.remove(&id) else {
            return;
        };
        let at = self.written.iter().position(|t| *t == id);
        if let Some(at) = at {
            self.written.remove(at);
        }
        if at == Some(0) {
            if let Some(next) = self
                .written
                .front()
                .and_then(|next| self.waiting.get_mut(next))
            {
                next.deadline = now + self.render_ms;
            }
        }
        match message.get("svg").and_then(|svg| svg.as_str()) {
            Some(svg) => {
                let key = cache_key(asked.kind, &asked.source, &asked.theme, asked.display);
                self.store(key.clone(), svg.to_string());
                self.disk.put(&key, svg, now);
                self.done(id, svg.to_string(), String::new());
            }
            None => {
                let error = message
                    .get("error")
                    .and_then(|e| e.as_str())
                    .filter(|e| !e.is_empty())
                    .unwrap_or("render failed");
                self.done(id, String::new(), error.to_string());
            }
        }
        self.arm_idle(now);
    }

    /// A document with figures is on screen and none is asked for yet: the helper starts now so it is warm when they are.
    pub fn warm(
        &mut self,
        figures: &[(FigureKind, String)],
        themes: &dyn Fn(FigureKind) -> Option<Theme>,
        now: u64,
    ) -> bool {
        if !self.available || self.stopping || self.starting || self.running {
            return false;
        }
        let mut kinds: Vec<&'static str> = Vec::new();
        let mut keys = Vec::new();
        for (kind, source) in figures {
            // The preview asks every figure as a display figure; a kind with no theme has no key and so is never known.
            keys.push(
                themes(*kind)
                    .map_or_else(String::new, |theme| cache_key(*kind, source, &theme, true)),
            );
            if !kinds.contains(&kind.name()) {
                kinds.push(kind.name());
            }
        }
        if kinds.is_empty() {
            return false;
        }
        self.warm_kinds = kinds;
        self.warm_keys = keys.clone();
        // A document whose figures are all on disk likely needs no helper, so the cache is asked before one is started.
        if self.disk.available {
            self.seq += 1;
            self.warm_query = self.seq;
            for event in self.disk.ask(self.warm_query, &keys, now) {
                self.store_event(event, now);
            }
            return true;
        }
        self.ensure_helper()
    }

    /// One event from the store's process, routed to its client.
    pub fn store_event(&mut self, event: StoreEvent, now: u64) {
        match event {
            StoreEvent::Answered { id, svg } => self.store_answered(id, svg, now),
            StoreEvent::Known { id, all } => self.warm_answered(id, all, now),
        }
    }

    fn warm_answered(&mut self, id: u64, all: bool, now: u64) {
        if id != self.warm_query {
            return;
        }
        self.warm_query = 0;
        if !all && !self.stopping && !self.starting && !self.running && self.waiting.is_empty() {
            self.ensure_helper();
        } else if all {
            self.warm_kinds.clear();
        }
        self.arm_idle(now);
    }

    fn store_answered(&mut self, id: u64, svg: String, now: u64) {
        let Some(asked) = self.waiting.get_mut(&id).filter(|w| w.on_disk) else {
            return;
        };
        asked.on_disk = false;
        if !svg.is_empty() {
            let Some(asked) = self.waiting.remove(&id) else {
                return;
            };
            self.store(
                cache_key(asked.kind, &asked.source, &asked.theme, asked.display),
                svg.clone(),
            );
            self.done(id, svg, String::new());
            self.arm_idle(now);
        } else if !self.available {
            self.waiting.remove(&id);
            self.done(id, String::new(), NOT_STARTED.to_string());
        } else {
            self.dispatch(id, now);
        }
    }

    /// A line from the store's process.
    pub fn store_line(&mut self, line: &str, now: u64) {
        for event in self.disk.receive(line) {
            self.store_event(event, now);
        }
    }

    /// The store's process is up, could not start, or exited.
    pub fn store_started(&mut self, now: u64) {
        self.disk.started(now);
    }

    pub fn store_spawn_failed(&mut self, now: u64) {
        for event in self.disk.spawn_failed(now) {
            self.store_event(event, now);
        }
    }

    pub fn store_exited(&mut self, now: u64) {
        for event in self.disk.exited(now) {
            self.store_event(event, now);
        }
    }
}
