//! The window's one file selection: what it published, what a watcher or foreign read reports, and which late reply may still land; a port of ui/js/Clipboard.js where dual panes share one session per backend id.
use super::paste::PendingPaste;
use super::entry::EntryHandle;
use super::wire::{clip_clear_line, clip_get_line, clip_set_line};
use super::{words, Out, Said};
use crate::i18n::I18n;
use std::collections::HashSet;

// Sample input: { paths: [], moving: false, token: "" } is the empty selection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clip {
    pub paths: Vec<String>,
    pub moving: bool,
    pub token: String,
}

impl Clip {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn file_op(&self) -> &'static str {
        if self.moving {
            "cut"
        } else {
            "copy"
        }
    }
}

/// One unanswered publish, carrying its entry identity so a late token can reach live captures without stamping a newer selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetEntry {
    pub backend: u64,
    pub clip: Clip,
    entry: EntryHandle,
}

/// One unanswered fallback read, whose reply may still serve a waiting paste.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetWait {
    pub backend: u64,
    pub generation: u64,
    pub sequence: u64,
}

/// A paste held for a fallback read, with the destination it captured; it fires from whatever the selection is when the read answers, as ready does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteWait {
    pub backend: u64,
    pub pending: PendingPaste,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Deferred {
    clip: String,
    paths: Vec<String>,
    token: String,
}

/// Whether a paste may run now or must wait for a fallback read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadOut {
    Now,
    Wait,
    Busy(Said),
}

#[derive(Default)]
pub struct Session {
    current: Clip,
    image_available: bool,
    current_entry: EntryHandle,
    sets: Vec<SetEntry>,
    own_tokens: HashSet<String>,
    generation: u64,
    revision: u64,
    failed: Vec<u64>,
    gets: Vec<GetWait>,
    paste_waits: Vec<PasteWait>,
    deferred: Option<Deferred>,
    get_sequence: u64,
    accepted_get_sequence: u64,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> &Clip {
        &self.current
    }

    /// Holds this entry and its eventual token until the paste completes or is cancelled.
    pub fn capture(&self) -> EntryHandle {
        self.current_entry.clone()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Moves whenever the selection's paths or cut intent change, including a fallback read's replacement, so a lookup keyed by it is never stale.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn watch_failed(&self, backend: u64) -> bool {
        self.failed.contains(&backend)
    }

    pub fn available(&self) -> bool {
        !self.current.is_empty()
    }

    pub fn image_available(&self) -> bool {
        self.image_available
    }

    pub fn pending_sets(&self) -> usize {
        self.sets.len()
    }

    pub fn pending_gets(&self) -> usize {
        self.gets.len()
    }

    /// A backend cannot answer after it ends; its replacement must read afresh.
    pub fn disconnected(&mut self, backend: u64) {
        self.sets.retain(|entry| entry.backend != backend);
        self.gets.retain(|wait| wait.backend != backend);
        self.paste_waits.retain(|wait| wait.backend != backend);
        if let Some(deferred) = self.deferred.take() {
            self.selection(&deferred.clip, deferred.paths, deferred.token);
        }
        if !self.failed.contains(&backend) {
            self.failed.push(backend);
        }
    }

    fn take_set(&mut self, backend: u64) -> Option<SetEntry> {
        self.sets
            .iter()
            .position(|entry| entry.backend == backend)
            .map(|at| self.sets.remove(at))
    }

    fn take_get(&mut self, backend: u64) -> Option<GetWait> {
        self.gets
            .iter()
            .position(|wait| wait.backend == backend)
            .map(|at| self.gets.remove(at))
    }

    fn take_paste_wait(&mut self, backend: u64) -> Option<PasteWait> {
        self.paste_waits
            .iter()
            .position(|wait| wait.backend == backend)
            .map(|at| self.paste_waits.remove(at))
    }

    fn replace(clip: &str, paths: Vec<String>, token: String) -> Clip {
        match clip {
            "copy" | "cut" => Clip { moving: clip == "cut", paths, token },
            _ => Clip::empty(),
        }
    }

    // A watcher echo of a publish still in flight: answered later, applied then.
    fn pending_echo(&self, clip: &str, paths: &[String], token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        self.sets.iter().any(|entry| {
            entry.clip.moving == (clip == "cut")
                && entry.clip.paths.len() == paths.len()
                && entry.clip.paths.iter().zip(paths.iter()).all(|(a, b)| a == b)
        })
    }

    fn selection(&mut self, clip: &str, paths: Vec<String>, token: String) {
        self.deferred = None;
        if !token.is_empty() && self.own_tokens.contains(&token) {
            return;
        }
        if self.pending_echo(clip, &paths, &token) {
            self.deferred = Some(Deferred { clip: clip.into(), paths, token });
            return;
        }
        self.generation += 1;
        self.revision += 1;
        self.current = Self::replace(clip, paths, token);
        self.image_available = clip == "image";
        self.current_entry = EntryHandle::new(&self.current.token);
        // Another owner holds the clipboard now, so a later offer of an older own token is that owner's choice to apply; sets still in flight have no token yet and add theirs when answered.
        self.own_tokens.clear();
    }

    // Sample input: copy ["/a/f1"] publishes clipSet and keeps the paths locally at once.
    pub fn set(&mut self, backend: u64, paths: Vec<String>, moving: bool) -> Vec<Out> {
        self.generation += 1;
        self.revision += 1;
        self.deferred = None;
        let clip = Clip { paths, moving, token: String::new() };
        let line = clip_set_line(clip.file_op(), &clip.paths);
        self.current = clip.clone();
        self.image_available = false;
        self.current_entry = EntryHandle::default();
        self.sets.push(SetEntry {
            backend,
            clip,
            entry: self.current_entry.clone(),
        });
        vec![Out::Send(line)]
    }

    // An answered set stamps its shared entry, but can publish only while that entry remains current.
    pub fn on_set(
        &mut self,
        i18n: &I18n,
        backend: u64,
        ok: bool,
        token: &str,
        error: &str,
    ) -> (Vec<Out>, Option<Clip>) {
        let Some(entry) = self.take_set(backend) else {
            return (Vec::new(), None);
        };
        let mut out = Vec::new();
        let mut publish = None;
        if !ok {
            out.push(Out::Say(Said { text: words::set_failed(i18n, error), error: true, paste_hint: false }));
        } else {
            entry.entry.stamp(token);
            if !token.is_empty() {
                self.own_tokens.insert(token.to_string());
            }
            if self.current_entry == entry.entry {
                self.current.token = token.to_string();
                publish = Some(self.current.clone());
            }
        }
        if let Some(deferred) = self.deferred.take() {
            self.selection(&deferred.clip, deferred.paths, deferred.token);
        }
        (out, publish)
    }

    // Sample input: an errored watch fails the backend but keeps the local copy.
    pub fn on_changed(
        &mut self,
        backend: u64,
        clip: &str,
        paths: Vec<String>,
        token: String,
        error: &str,
    ) -> Vec<Out> {
        if !error.is_empty() {
            if !self.failed.contains(&backend) {
                self.failed.push(backend);
            }
            return Vec::new();
        }
        if let Some(at) = self.failed.iter().position(|id| *id == backend) {
            self.failed.remove(at);
        }
        self.selection(clip, paths, token);
        Vec::new()
    }

    // A foreign selection the host already ordered in the fork's own monotonic generation: apply it subject to the own-token and echo rules.
    pub fn foreign(&mut self, clip: &str, paths: Vec<String>, token: String) {
        self.selection(clip, paths, token);
    }

    // A paste reads the system selection first only while its own watch is down.
    pub fn read_for_paste(&mut self, i18n: &I18n, backend: u64, pending: PendingPaste) -> (Vec<Out>, ReadOut) {
        if !self.sets.is_empty() || !self.failed.contains(&backend) {
            return (Vec::new(), ReadOut::Now);
        }
        if self.paste_waits.iter().any(|wait| wait.backend == backend) {
            let said = Said { text: words::reading(i18n), error: false, paste_hint: false };
            return (vec![Out::Say(said.clone())], ReadOut::Busy(said));
        }
        self.get_sequence += 1;
        let wait = GetWait { backend, generation: self.generation, sequence: self.get_sequence };
        self.gets.push(wait);
        self.paste_waits.push(PasteWait { backend, pending });
        (vec![Out::Send(clip_get_line())], ReadOut::Wait)
    }

    // A newer local set, a newer watcher selection or a later accepted read outranks this reply.
    pub fn on_get(
        &mut self,
        backend: u64,
        ok: bool,
        clip: &str,
        paths: Vec<String>,
        token: String,
    ) -> (Vec<Out>, Vec<PendingPaste>) {
        let Some(waiting) = self.take_get(backend) else {
            return (Vec::new(), Vec::new());
        };
        let mut pendings = Vec::new();
        if let Some(wait) = self.take_paste_wait(backend) {
            pendings.push(wait.pending);
        }
        if ok
            && self.sets.is_empty()
            && waiting.generation == self.generation
            && waiting.sequence > self.accepted_get_sequence
        {
            self.accepted_get_sequence = waiting.sequence;
            if !self.own_tokens.contains(&token) {
                self.own_tokens.clear();
            }
            self.current = Self::replace(clip, paths, token);
            self.image_available = clip == "image";
            self.current_entry = EntryHandle::new(&self.current.token);
            self.revision += 1;
        }
        (Vec::new(), pendings)
    }

    // A failed clear names the system clipboard; a refused one is still said.
    pub fn on_clear(&mut self, i18n: &I18n, ok: bool, error: &str) -> Vec<Out> {
        if ok {
            return Vec::new();
        }
        vec![Out::Say(Said { text: words::clear_failed(i18n, error), error: true, paste_hint: false })]
    }

    // Every capture shares its entry's eventual token; only a matching entry can empty the current selection.
    pub fn spent(&mut self, captured: &Clip, entry: &EntryHandle, spends_cut: bool) -> Vec<Out> {
        if !(spends_cut && captured.moving) {
            return Vec::new();
        }
        let same_entry = self.current_entry == *entry;
        let token = if entry.token().is_empty() { &captured.token } else { entry.token() };
        let mut out = Vec::new();
        if !token.is_empty() {
            out.push(Out::Send(clip_clear_line(token, &[])));
        } else {
            out.push(Out::Send(clip_clear_line("", &captured.paths)));
        }
        if same_entry {
            self.current = Clip::empty();
            self.current_entry = EntryHandle::default();
            self.revision += 1;
        }
        out
    }
}

#[cfg(test)]
#[path = "retention_tests.rs"]
mod retention_tests;

#[cfg(test)]
#[path = "image_tests.rs"]
mod image_tests;
