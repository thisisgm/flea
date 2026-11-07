//! Clipboard file selections over the G1 shell: one selection, one queue per pane backend and the cut marks rows draw; the core flea::ui_logic_dg1 decides, and this host drains its outbox to the wire, sink, owner and status slot.
pub mod carry;
mod carry_landing;
pub mod flock;
pub mod host;
pub(crate) mod ipc;
pub mod join;
mod keyed;
pub mod keys;
pub mod marks;
mod receive;
pub mod seams;
mod transport;

#[cfg(test)]
mod host_marks_tests;
#[cfg(test)]
mod host_races_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod join_tests;
#[cfg(test)]
mod m9_g2rows_25_carry_tests;
#[cfg(test)]
mod m9_g2rows_25_rig;
#[cfg(test)]
mod m9_g2rows_25_target_lifetime_tests;
#[cfg(test)]
mod m9_g2rows_25_target_rig;
#[cfg(test)]
mod m9_g2rows_25_target_tests;
#[cfg(test)]
mod relative_tests;
#[cfg(test)]
mod review_r4_tests;
#[cfg(test)]
mod rig;
#[cfg(test)]
mod tests;

use flea::i18n::I18n;
use flea::ui_logic_dg1::{self as core};
use flea::ui_logic_mn1::copyas::Format;
use seams::{Captured, Owner, Sink, Wire};
use std::collections::{HashMap, HashSet};

/// What a take needs from the pane: its path, its held listing, and whether another asker holds the shared paths slot (the G1 pane owns that slot).
#[derive(Clone, Copy, Debug)]
pub struct Take<'a> {
    pub path: &'a str,
    pub listing: u64,
    pub paths_busy: bool,
    pub list_in_flight: bool,
}

/// What a paste needs from the pane when its read answers.
#[derive(Clone, Copy, Debug)]
pub struct Landing<'a> {
    pub path: &'a str,
    pub list_in_flight: bool,
    pub recent: bool,
    pub stamp: core::Stamp,
}

// The core queue checks landing requests against these same facts.
fn view(take: Take<'_>) -> core::View<'_> {
    core::View {
        path: take.path,
        listing: take.listing,
        paths_busy: take.paths_busy,
        list_in_flight: take.list_in_flight,
    }
}

/// The window's clipboard: one session, one queue per backend, one mark table.
pub struct Clipboard {
    session: core::Session,
    queues: HashMap<u64, core::Queue>,
    // A pending Copy As per backend, with the folder Relative Path reads from, kept so a later listing cannot change it.
    copy_as: HashMap<u64, (Format, String)>,
    marks: core::Marks,
    /// The buffer a row's path joins into, reused so a row probe allocates nothing once warm.
    probe: String,
    watched: HashSet<u64>,
    foreign_generation: Option<u64>,
    /// Times the mark lookup was rebuilt, for tests and the headless gate.
    pub builds: u64,
    /// Rows that asked for a mark, for tests and the headless gate.
    pub calls: u64,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            session: core::Session::new(),
            queues: HashMap::new(),
            copy_as: HashMap::new(),
            marks: core::Marks::new(),
            probe: String::new(),
            watched: HashSet::new(),
            foreign_generation: None,
            builds: 0,
            calls: 0,
        }
    }

    // Each writable backend watches once at start, as Backend.qml does.
    pub fn watch(&mut self, backend: u64, wire: &dyn Wire) {
        if self.watched.insert(backend) {
            self.session
                .on_changed(backend, "none", Vec::new(), String::new(), "unwatched");
            wire.send(core::clip_watch_line());
        }
    }

    // A watch begun at first use: until its first changed line the backend reads as down, so a paste reads the system selection first and the menu keeps Paste enabled.
    pub fn watch_unknown(&mut self, backend: u64, wire: &dyn Wire) {
        self.watch(backend, wire);
    }

    pub fn watching(&self, backend: u64) -> bool {
        self.watched.contains(&backend)
    }

    pub fn current(&self) -> &core::Clip {
        self.session.current()
    }

    /// Moves with every change of the selection's paths or cut intent.
    pub fn revision(&self) -> u64 {
        self.session.revision()
    }

    pub fn unwatch(&mut self, backend: u64) {
        self.session.disconnected(backend);
        self.watched.remove(&backend);
        self.queues.remove(&backend);
        self.copy_as.remove(&backend);
    }

    pub fn available(&self) -> bool {
        self.session.available()
    }

    pub fn image_available(&self) -> bool {
        self.session.image_available()
    }

    pub fn watch_failed(&self, backend: u64) -> bool {
        self.session.watch_failed(backend)
    }

    /// Whether this backend holds the shared paths slot, a rows take or a Copy As round trip; G1's other askers read it to refuse while dg1 answers.
    pub fn paths_pending(&self, backend: u64) -> bool {
        self.copy_as.contains_key(&backend)
            || self
                .queues
                .get(&backend)
                .is_some_and(|queue| queue.pending())
    }

    pub fn token(&self) -> &str {
        &self.session.current().token
    }

    fn drain(
        &mut self,
        outs: Vec<core::Out>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        let mut said = Vec::new();
        for out in outs {
            match out {
                core::Out::Send(line) => {
                    wire.send(line);
                }
                core::Out::Say(line) => said.push(line),
                core::Out::AskPaths(rows) => {
                    if !rows.is_empty() {
                        wire.send(core::paths_request_line(&rows));
                    }
                }
                core::Out::Ask(ask) => {
                    let current = self.session.current().clone();
                    let entry = self.session.capture();
                    sink.paste(
                        ask,
                        Captured {
                            paths: current.paths,
                            moving: current.moving,
                            token: current.token,
                            entry,
                        },
                    );
                }
                core::Out::Text(text) => {
                    owner.publish_text(text);
                }
            }
        }
        said
    }

    // Copy or cut rows, or publish absolute paths at once when the menu took them; empty rows publish nothing and the caller says the no-target line, since cursor and filter belong to the G1 listing. Nine parameters: each is an independent seam (src/keys.rs uses the same shape).
    #[allow(clippy::too_many_arguments)]
    pub fn take(
        &mut self,
        i18n: &I18n,
        backend: u64,
        moving: bool,
        paths: Option<Vec<String>>,
        rows: Vec<usize>,
        take: Take<'_>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        if paths.is_none() && rows.is_empty() {
            return Vec::new();
        }
        let busy = take.paths_busy || (paths.is_none() && self.copy_as.contains_key(&backend));
        let view = core::View {
            path: take.path,
            listing: take.listing,
            paths_busy: busy,
            list_in_flight: take.list_in_flight,
        };
        let out = self
            .queues
            .entry(backend)
            .or_default()
            .take(i18n, moving, paths, rows, view);
        match out {
            core::TakeOut::Publish(paths, moving) => {
                let outs = self.session.set(backend, paths.clone(), moving);
                let mut said = self.drain(outs, wire, sink, owner);
                said.push(core::landed(i18n, moving, paths.len()));
                said
            }
            core::TakeOut::AskPaths(rows) => {
                self.drain(vec![core::Out::AskPaths(rows)], wire, sink, owner)
            }
            core::TakeOut::Wait => Vec::new(),
            core::TakeOut::Busy(said) => vec![said],
        }
    }

    // The backend answered a paths request: only this host's pending take or Copy As may claim it, never another asker's.
    #[allow(clippy::too_many_arguments)]
    pub fn paths_resolved(
        &mut self,
        i18n: &I18n,
        backend: u64,
        list: Vec<String>,
        take: Take<'_>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        if let Some((format, base)) = self.copy_as.remove(&backend) {
            return match core::copy_as_resolved(&list, format, &base) {
                core::CopyAsOut::Text(text) => {
                    self.drain(vec![core::Out::Text(text)], wire, sink, owner)
                }
                _ => Vec::new(),
            };
        }
        let mut said = Vec::new();
        match self
            .queues
            .get_mut(&backend)
            .and_then(|queue| queue.resolved(i18n, list, view(take)))
        {
            Some(Ok(publish)) => {
                let outs = self
                    .session
                    .set(backend, publish.paths.clone(), publish.moving);
                said.extend(self.drain(outs, wire, sink, owner));
                said.push(core::landed(i18n, publish.moving, publish.paths.len()));
            }
            Some(Err(stale)) => said.push(stale),
            None => {}
        }
        // A superseded reply still drains: the newest queued take asks its own rows (ClipSelection.resolved always drains, orderedPicks).
        let next = self
            .queues
            .get_mut(&backend)
            .map(|queue| queue.step(i18n, view(take)))
            .unwrap_or(core::TakeOut::Wait);
        match next {
            core::TakeOut::AskPaths(rows) if !rows.is_empty() => {
                said.extend(self.drain(vec![core::Out::AskPaths(rows)], wire, sink, owner));
            }
            core::TakeOut::Busy(line) => said.push(line),
            _ => {}
        }
        said
    }

    // A failed paths request, or a listing that ended under it, ends its claim so the next copy is never blocked.
    pub fn paths_failed(
        &mut self,
        i18n: &I18n,
        backend: u64,
        take: Take<'_>,
        wire: &dyn Wire,
        sink: &mut dyn Sink,
        owner: &mut dyn Owner,
    ) -> Vec<core::Said> {
        self.copy_as.remove(&backend);
        let queue = self.queues.entry(backend).or_default();
        queue.failed();
        match queue.step(i18n, view(take)) {
            core::TakeOut::AskPaths(rows) if !rows.is_empty() => {
                self.drain(vec![core::Out::AskPaths(rows)], wire, sink, owner)
            }
            core::TakeOut::Busy(line) => vec![line],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod m9_g2rows_25_pending_tests;
