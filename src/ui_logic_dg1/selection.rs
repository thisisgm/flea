//! The copy/cut queue: one paths round trip serves one request at a time and only the newest choice publishes; a port of ui/js/ClipSelection.js, where a landed listing must still stand or the operator is told to copy or cut again.
use super::{words, Said};
use crate::i18n::I18n;

// Sample input: sequence 7 over rows [0] in listing 1 of "/dest".
#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    sequence: u64,
    moving: bool,
    paths: Option<Vec<String>>,
    rows: Vec<usize>,
    path: String,
    listing: u64,
}

/// What `take` decided: publish at once, ask the backend, wait, or refuse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TakeOut {
    Publish(Vec<String>, bool),
    AskPaths(Vec<usize>),
    Wait,
    Busy(Said),
}

/// What a resolved paths reply publishes, if its request is still the newest.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Publish {
    pub paths: Vec<String>,
    pub moving: bool,
}

/// The pane facts a request is checked against when taken and when it lands; paths_busy refuses a rows take while another asker holds the shared paths slot, and list_in_flight makes a landing request stale.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub path: &'a str,
    pub listing: u64,
    pub paths_busy: bool,
    pub list_in_flight: bool,
}

#[derive(Default)]
pub struct Queue {
    sequence: u64,
    queue: Vec<Request>,
    pending: Option<Request>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    // Copy or cut rows, or publish absolute paths at once when the menu took them.
    pub fn take(
        &mut self,
        i18n: &I18n,
        moving: bool,
        paths: Option<Vec<String>>,
        rows: Vec<usize>,
        view: View<'_>,
    ) -> TakeOut {
        if paths.is_none() && view.paths_busy {
            return TakeOut::Busy(Said { text: words::resolving(i18n), error: false, paste_hint: false });
        }
        self.sequence += 1;
        let request = Request {
            sequence: self.sequence,
            moving,
            rows: if paths.is_some() { Vec::new() } else { rows },
            paths: paths.clone(),
            path: view.path.into(),
            listing: view.listing,
        };
        self.queue.push(request);
        if paths.is_some() {
            // A publish drains too: published and superseded entries leave, and no older rows request can start behind the newest choice.
            let _ = self.step(i18n, view);
            return TakeOut::Publish(paths.unwrap_or_default(), moving);
        }
        self.step(i18n, view)
    }

    // After a reply or a failure, serve the next queued request, if any.
    pub fn step(&mut self, i18n: &I18n, view: View<'_>) -> TakeOut {
        if self.pending.is_some() {
            return TakeOut::Wait;
        }
        while let Some(first) = self.queue.first().cloned() {
            if first.paths.is_some() || first.sequence != self.sequence {
                self.queue.remove(0);
                continue;
            }
            if first.path != view.path || first.listing != view.listing || view.list_in_flight {
                self.queue.remove(0);
                return TakeOut::Busy(Said { text: words::stale(i18n), error: false, paste_hint: false });
            }
            self.pending = Some(first.clone());
            return TakeOut::AskPaths(first.rows);
        }
        TakeOut::Wait
    }

    // The backend answered askPaths: only the newest choice publishes, as publish refuses a superseded sequence, and a landing whose listing moved or is reloading is dropped with the backend's stale refusal, as v0.3.12 says it for a sent request; the host calls step after either to serve what is queued.
    pub fn resolved(&mut self, i18n: &I18n, list: Vec<String>, view: View<'_>) -> Option<Result<Publish, Said>> {
        let pending = self.pending.take()?;
        if pending.sequence != self.sequence {
            self.queue.retain(|request| request.sequence != pending.sequence);
            return None;
        }
        if pending.path != view.path || pending.listing != view.listing || view.list_in_flight {
            self.queue.retain(|request| request.sequence != pending.sequence);
            return Some(Err(words::landed_stale(i18n)));
        }
        let found = self
            .queue
            .iter_mut()
            .find(|request| request.sequence == pending.sequence)?;
        found.paths = Some(list.clone());
        Some(Ok(Publish { paths: list, moving: found.moving }))
    }

    pub fn failed(&mut self) {
        if let Some(pending) = self.pending.take() {
            self.queue.retain(|request| request.sequence != pending.sequence);
        }
    }
}
