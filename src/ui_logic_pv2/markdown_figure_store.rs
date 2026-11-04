//! The persistent figure cache's client: one JSON line per request to `flea --figure-store`, every failure a miss.

use super::markdown_figure_service::Command;
use crate::json::escape;
use std::collections::BTreeMap;

/// A reply later than this ends the store, and its figures draw by the helper.
pub const REPLY_MS: u64 = 2000;
/// A request queued behind a drain or a start waits this long, then it is a miss.
pub const DRAIN_MS: u64 = REPLY_MS;
/// A drain still running at this bound is a hung store and is killed, never latched.
pub const DRAIN_HANG_MS: u64 = 60_000;

/// What the store tells the service: a get's picture ("" for a miss), or whether a warm query's figures are all held.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreEvent {
    Answered { id: u64, svg: String },
    Known { id: u64, all: bool },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    Get,
    Known,
}

struct Owed {
    op: Op,
    queued_at: u64,
    sent_at: Option<u64>,
}

/// The store client's state; the service drives it and takes its commands.
pub struct Store {
    pub available: bool,
    pub reply_ms: u64,
    pub drain_ms: u64,
    pub drain_hang_ms: u64,
    pub hang_kills: usize,
    pub hits: usize,
    pub misses: usize,
    pub puts: usize,
    pub exits: usize,
    running: bool,
    starting: bool,
    stopping: bool,
    hang_at: Option<u64>,
    queued: Vec<(String, Option<u64>)>,
    outstanding: BTreeMap<u64, Owed>,
    commands: Vec<Command>,
}

impl Default for Store {
    fn default() -> Self {
        Store {
            available: true,
            reply_ms: REPLY_MS,
            drain_ms: DRAIN_MS,
            drain_hang_ms: DRAIN_HANG_MS,
            hang_kills: 0,
            hits: 0,
            misses: 0,
            puts: 0,
            exits: 0,
            running: false,
            starting: false,
            stopping: false,
            hang_at: None,
            queued: Vec::new(),
            outstanding: BTreeMap::new(),
            commands: Vec::new(),
        }
    }
}

fn miss(id: u64, op: Op) -> StoreEvent {
    match op {
        Op::Get => StoreEvent::Answered {
            id,
            svg: String::new(),
        },
        Op::Known => StoreEvent::Known { id, all: false },
    }
}

impl Store {
    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    /// The process runs or is starting.
    pub fn active(&self) -> bool {
        self.running || self.starting
    }

    /// The replies still owed.
    pub fn owed(&self) -> usize {
        self.outstanding.len()
    }

    /// When a queued line lapses, a sent one times out or a drain counts as hung.
    pub fn next_wake(&self) -> Option<u64> {
        let owed = self.outstanding.values().map(|o| match o.sent_at {
            None => o.queued_at + self.drain_ms + 1,
            Some(sent) => sent + self.reply_ms + 1,
        });
        owed.chain(self.hang_at).min()
    }

    /// Lapses the lines that waited past the drain, fails a store that did not reply, and kills a hung drain.
    pub fn tick(&mut self, now: u64) -> Vec<StoreEvent> {
        if self.hang_at.is_some_and(|at| now >= at) {
            return self.hang();
        }
        let mut lapsed = Vec::new();
        for (id, asked) in &self.outstanding {
            match asked.sent_at {
                None if now - asked.queued_at.min(now) > self.drain_ms => lapsed.push(*id),
                Some(sent) if now - sent.min(now) > self.reply_ms => return self.fail(now),
                _ => {}
            }
        }
        lapsed.into_iter().map(|id| self.lapse(id)).collect()
    }

    fn release(&mut self) -> Vec<StoreEvent> {
        std::mem::take(&mut self.outstanding)
            .into_iter()
            .map(|(id, owed)| miss(id, owed.op))
            .collect()
    }

    // Latches off, ends the process and releases everything waiting as a miss.
    fn fail(&mut self, now: u64) -> Vec<StoreEvent> {
        self.starting = false;
        self.available = false;
        self.queued.clear();
        if self.running {
            self.begin_stopping(now);
            self.commands.push(Command::StoreKill);
        }
        self.release()
    }

    // The drain's hang bound runs only while the store is stopping, and a drain that ends restarts it.
    fn begin_stopping(&mut self, now: u64) {
        if !self.stopping {
            self.stopping = true;
            self.hang_at = Some(now + self.drain_hang_ms);
        }
    }

    fn lapse(&mut self, id: u64) -> StoreEvent {
        let op = self.outstanding.remove(&id).map_or(Op::Get, |owed| owed.op);
        self.queued.retain(|(_, queued)| *queued != Some(id));
        miss(id, op)
    }

    fn hang(&mut self) -> Vec<StoreEvent> {
        self.hang_kills += 1;
        self.hang_at = None;
        if self.running {
            self.commands.push(Command::StoreKill);
        }
        self.queued.retain(|(_, id)| id.is_none());
        self.release()
    }

    /// An idle stop closes the input so the store drains and exits; it is refused while anything waits.
    pub fn stop(&mut self, now: u64) -> bool {
        if !self.running
            || self.starting
            || self.stopping
            || !self.outstanding.is_empty()
            || !self.queued.is_empty()
        {
            return false;
        }
        self.begin_stopping(now);
        self.commands.push(Command::StoreCloseInput);
        true
    }

    fn begin(&mut self) {
        self.starting = true;
        self.commands.push(Command::StoreSpawn);
    }

    fn write(&mut self, line: String, id: Option<u64>, now: u64) {
        self.commands.push(Command::StoreWrite(line));
        if let Some(owed) = id.and_then(|id| self.outstanding.get_mut(&id)) {
            owed.sent_at = Some(now);
        }
    }

    fn send(&mut self, line: String, id: Option<u64>, now: u64) -> bool {
        if !self.available {
            return false;
        }
        if self.running && !self.starting && !self.stopping {
            self.write(line, id, now);
            return true;
        }
        self.queued.push((line, id));
        if !self.starting && !self.stopping {
            self.begin();
        }
        true
    }

    fn owe(&mut self, id: u64, op: Op, now: u64) {
        self.outstanding.insert(
            id,
            Owed {
                op,
                queued_at: now,
                sent_at: None,
            },
        );
    }

    /// {"op":"get","id":3,"key":"math\n...\ntrue\nx^2"}; the answer is an Answered event, at once when the store is off.
    pub fn get(&mut self, id: u64, key: &str, now: u64) -> Vec<StoreEvent> {
        self.owe(id, Op::Get, now);
        let line = format!(
            "{{\"op\":\"get\",\"id\":{id},\"key\":\"{}\"}}\n",
            escape(key)
        );
        if self.send(line, Some(id), now) {
            return Vec::new();
        }
        self.outstanding.remove(&id);
        vec![StoreEvent::Answered {
            id,
            svg: String::new(),
        }]
    }

    /// {"op":"put","key":"math\n...","svg":"<svg ...>"}, with no reply.
    pub fn put(&mut self, key: &str, svg: &str, now: u64) {
        let line = format!(
            "{{\"op\":\"put\",\"key\":\"{}\",\"svg\":\"{}\"}}\n",
            escape(key),
            escape(svg)
        );
        if self.send(line, None, now) {
            self.puts += 1;
        }
    }

    /// {"op":"known","id":4,"keys":[...]}; the answer is a Known event, at once and false when the store is off.
    pub fn ask(&mut self, id: u64, keys: &[String], now: u64) -> Vec<StoreEvent> {
        self.owe(id, Op::Known, now);
        let keys: Vec<String> = keys
            .iter()
            .map(|key| format!("\"{}\"", escape(key)))
            .collect();
        let line = format!(
            "{{\"op\":\"known\",\"id\":{id},\"keys\":[{}]}}\n",
            keys.join(",")
        );
        if self.send(line, Some(id), now) {
            return Vec::new();
        }
        self.outstanding.remove(&id);
        vec![StoreEvent::Known { id, all: false }]
    }

    /// The process is up: the queued lines go out in order.
    pub fn started(&mut self, now: u64) {
        self.starting = false;
        self.running = true;
        if self.stopping {
            self.commands.push(Command::StoreKill);
            return;
        }
        for (line, id) in std::mem::take(&mut self.queued) {
            self.write(line, id, now);
        }
    }

    /// The process could not be started, which latches the store off.
    pub fn spawn_failed(&mut self, now: u64) -> Vec<StoreEvent> {
        if self.starting && !self.stopping {
            return self.fail(now);
        }
        Vec::new()
    }

    /// The process exited: an exit nobody asked for latches the store off, and lines sent during a drain start it again.
    pub fn exited(&mut self, now: u64) -> Vec<StoreEvent> {
        self.exits += 1;
        self.running = false;
        let was_stopping = self.stopping;
        self.starting = false;
        self.stopping = false;
        self.hang_at = None;
        if !was_stopping {
            return self.fail(now);
        }
        if !self.queued.is_empty() {
            self.begin();
        }
        Vec::new()
    }

    /// {"id":3,"svg":"<svg ...>"}, {"id":3,"miss":true} or {"id":4,"known":true}.
    pub fn receive(&mut self, line: &str) -> Vec<StoreEvent> {
        let Ok(message) = crate::jsondoc::parse(line) else {
            return Vec::new();
        };
        let Some(id) = message
            .get("id")
            .and_then(|id| id.as_f64())
            .filter(|id| *id >= 0.0)
            .map(|id| id as u64)
        else {
            return Vec::new();
        };
        let Some(asked) = self.outstanding.remove(&id) else {
            return Vec::new();
        };
        if asked.op == Op::Known {
            return vec![StoreEvent::Known {
                id,
                all: message.get("known").and_then(|k| k.as_bool()) == Some(true),
            }];
        }
        match message.get("svg").and_then(|svg| svg.as_str()) {
            Some(svg) => {
                self.hits += 1;
                vec![StoreEvent::Answered {
                    id,
                    svg: svg.to_string(),
                }]
            }
            None => {
                self.misses += 1;
                vec![StoreEvent::Answered {
                    id,
                    svg: String::new(),
                }]
            }
        }
    }
}

#[cfg(test)]
#[path = "markdown_figure_store_tests.rs"]
mod tests;
