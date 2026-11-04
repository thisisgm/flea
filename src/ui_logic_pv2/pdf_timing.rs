//! When a PDF surface opens a document, falls back to its loading state and fetches a slow copy: the timing half of `pdf`.
use super::DOCUMENT_SETTLE_MS;

/// A request is due once the settle has passed since the last one, or the clock went back.
pub fn due(now: u64, last: Option<u64>, interval: u64) -> bool {
    match last {
        None => true,
        Some(last) => now < last || now - last >= interval,
    }
}

/// A PDF opens at once only when the cursor is idle and no page render is in flight.
pub fn open_now(now: u64, last: Option<u64>, interval: u64, render_busy: bool) -> bool {
    !render_busy && due(now, last, interval)
}

/// Which document the surface has opened, and how the next one is let in: at the settle, or on the next frame when idle.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Opener {
    pub opened: String,
    pub wanted: String,
    pub last_request_at: Option<u64>,
    pub fast_armed: bool,
    pub settle_at: Option<u64>,
    /// Opens taken by the fast path, so a test tells them from the settle by count, never by time.
    pub fast_opens: usize,
}

impl Opener {
    /// A path or active change; a change that opens nothing stamps nothing.
    pub fn request(&mut self, path: &str, active: bool, now: u64, render_busy: bool) {
        self.wanted = if active {
            path.to_string()
        } else {
            String::new()
        };
        if self.wanted == self.opened {
            return;
        }
        self.fast_armed = open_now(now, self.last_request_at, DOCUMENT_SETTLE_MS, render_busy);
        self.last_request_at = Some(now);
        self.settle_at = Some(now + DOCUMENT_SETTLE_MS);
    }

    /// The first presented frame after an armed press opens it; true when that changed the open document.
    pub fn frame(&mut self) -> bool {
        if !self.fast_armed {
            return false;
        }
        self.fast_armed = false;
        self.settle_at = None;
        self.fast_opens += 1;
        self.open_pending()
    }

    /// The settle timer; it opens whatever is wanted once its deadline has come.
    pub fn tick(&mut self, now: u64) -> bool {
        match self.settle_at {
            Some(at) if now >= at => {
                self.settle_at = None;
                self.fast_armed = false;
                self.open_pending()
            }
            _ => false,
        }
    }

    fn open_pending(&mut self) -> bool {
        if self.wanted == self.opened {
            return false;
        }
        self.opened = self.wanted.clone();
        true
    }
}

/// The slow render fall-back: a turn whose render outlasts the cap shows the loading state, which then stands its minimum.
#[derive(Clone, Debug, PartialEq)]
pub struct Cap {
    pub cap_ms: u64,
    pub min_show_ms: u64,
    pub cap_at: Option<u64>,
    pub min_at: Option<u64>,
    pub fell_back: bool,
    pub fell_back_at: Option<u64>,
    /// The page whose render is on screen, None until one has landed for this document.
    pub shown_page: Option<usize>,
}

impl Cap {
    pub fn new(cap_ms: u64, min_show_ms: u64) -> Self {
        Cap {
            cap_ms,
            min_show_ms,
            cap_at: None,
            min_at: None,
            fell_back: false,
            fell_back_at: None,
            shown_page: None,
        }
    }

    /// Another document's page is never this one's: everything is dropped before a switch.
    pub fn reset(&mut self) {
        *self = Cap::new(self.cap_ms, self.min_show_ms);
    }

    /// What the chrome counts: the page on screen, or the one asked for once nothing else stands in for it.
    pub fn drawn_page(&self, page: usize) -> usize {
        match self.shown_page {
            Some(shown) if !self.fell_back => shown,
            _ => page,
        }
    }

    /// A turn arms the cap for a page other than the one on screen; turning back onto the shown page stops it.
    pub fn page_changed(&mut self, page: usize, loading: bool, now: u64) {
        let Some(shown) = self.shown_page else { return };
        if !loading {
            return;
        }
        if page != shown {
            self.cap_at = Some(now + self.cap_ms);
        } else {
            self.cap_at = None;
            self.end_fall_back(now);
        }
    }

    /// A render of another page than the one on screen restarts the cap.
    pub fn render_started(&mut self, page: usize, now: u64) {
        if matches!(self.shown_page, Some(shown) if shown != page) {
            self.min_at = None;
            self.cap_at = Some(now + self.cap_ms);
        }
    }

    /// A landed or failed render ends the cap; only a landed one is on screen.
    pub fn render_finished(&mut self, landed: Option<usize>, now: u64) {
        self.cap_at = None;
        self.end_fall_back(now);
        self.shown_page = landed;
    }

    /// The loading state leaves once it has stood its minimum; until then the page waits behind it.
    pub fn end_fall_back(&mut self, now: u64) {
        let left = match (self.fell_back, self.fell_back_at) {
            (true, Some(at)) => (at + self.min_show_ms).saturating_sub(now),
            _ => 0,
        };
        if left > 0 {
            self.min_at = Some(now + left);
            return;
        }
        self.min_at = None;
        self.fell_back = false;
        self.fell_back_at = None;
    }

    /// The two timers; true when the drawn state changed.
    pub fn tick(&mut self, now: u64) -> bool {
        let before = self.fell_back;
        if matches!(self.cap_at, Some(at) if now >= at) {
            self.cap_at = None;
            if !self.fell_back {
                self.fell_back_at = Some(now);
            }
            self.fell_back = true;
        }
        if matches!(self.min_at, Some(at) if now >= at) {
            self.min_at = None;
            self.fell_back = false;
            self.fell_back_at = None;
        }
        before != self.fell_back
    }

    /// The next instant a timer is due, for the surface's one scheduled wake.
    pub fn next_deadline(&self) -> Option<u64> {
        match (self.cap_at, self.min_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

/// Where a slow storage document stands: none needed, out, landed as a local copy, or failed.
#[derive(Clone, Debug, PartialEq)]
pub enum Fetch {
    InPlace,
    Waiting(u64),
    Copied(String),
    Failed { stalled: bool },
}

impl Fetch {
    /// Decided once per opened document, so a later storage class change neither blanks nor reloads it.
    pub fn decide(opened: &str, fetch_first: bool, has_backend: bool) -> bool {
        !opened.is_empty() && fetch_first && has_backend
    }

    /// The path the document loads from: the copy once it lands, in place while no fetch runs, nothing while one is out.
    pub fn source<'a>(&'a self, opened: &'a str) -> Option<&'a str> {
        if opened.is_empty() {
            return None;
        }
        match self {
            Fetch::InPlace => Some(opened),
            Fetch::Copied(path) => Some(path),
            Fetch::Waiting(_) | Fetch::Failed { .. } => None,
        }
    }

    /// A copy reply: only the fetch in flight takes it; an error with the stalled flag is the not-responding failure.
    pub fn copied(&mut self, id: u64, path: &str, error: &str, stalled: bool) -> bool {
        if *self != Fetch::Waiting(id) {
            return false;
        }
        *self = if error.is_empty() {
            Fetch::Copied(path.to_string())
        } else {
            Fetch::Failed { stalled }
        };
        true
    }

    /// The catalog key of the one sentence a failed document gets: a fetch that never answered, or anything else.
    pub fn fail_key(&self) -> &'static str {
        match self {
            Fetch::Failed { stalled: true } => "document.pdf_not_responding",
            _ => "document.pdf_unreadable",
        }
    }
}

#[cfg(test)]
#[path = "pdf_timing_tests.rs"]
mod tests;
