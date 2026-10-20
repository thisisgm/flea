//! Feedback presentation shared by the GUI and the TUI: failure sentences, status messages, reload notices, the empty hero and refusals.
pub mod empty;
pub mod errors;
mod lane;
pub mod messages;
pub mod status;
#[cfg(test)]
mod tests;

use crate::i18n::I18n;

/// A reveal reads slower than a hide; both run on OutCubic.
pub const OPEN_MS: u64 = 180;
pub const CLOSE_MS: u64 = 140;
/// An opening surface rises into place from this far below; a close fades only.
pub const TRANSLATE_UP_PX: f32 = 10.;
/// How long the pointer rests at the left edge before the hidden rail opens.
pub const DWELL_MS: u64 = 120;

/// Omarchy's OutCubic curve over 0 to 1.
pub fn out_cubic(t: f32) -> f32 {
    let rest = 1. - t.clamp(0., 1.);
    1. - rest * rest * rest
}

/// The notice a manual reload answers with, counted in items like the status bar.
pub fn reload_line(changed: usize, catalog: &I18n) -> String {
    catalog.trn("feedback.reload", changed as i64)
}

/// What a reload request does with the pane as it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Begin {
    /// A listing is out, so the pane says it is loading and nothing else happens.
    Loading,
    /// A search owns the header, so the reload goes quiet.
    Quiet,
    /// The folder waited on is not the pane's path yet: list that folder without history.
    OpenWaited(String),
    /// Recent re-reads its history; the notice is owed.
    Recent,
    /// An anchored re-read of the pane's folder; the notice is owed.
    Reread,
}

/// `waiting_on` is the folder a waiting listing names, already trimmed of its trailing slash.
pub fn reload_begin(
    loading: bool,
    searching: bool,
    waiting_on: &str,
    path: &str,
    recent: bool,
) -> Begin {
    if loading {
        Begin::Loading
    } else if searching {
        Begin::Quiet
    } else if recent {
        Begin::Recent
    } else if !waiting_on.is_empty() && waiting_on != path {
        Begin::OpenWaited(waiting_on.into())
    } else {
        Begin::Reread
    }
}

/// A manual reload's debt: the count it started from and the backend's own changed count when one arrives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reload {
    from: Option<usize>,
    changed: Option<usize>,
}

impl Reload {
    /// Set after the re-read starts, because opening the listing clears it first.
    pub fn arm(&mut self, total: usize) {
        self.from = Some(total);
        self.changed = None;
    }

    /// The backend's added plus removed count for a same-path re-list.
    pub fn counted(&mut self, changed: usize) {
        if self.from.is_some() {
            self.changed = Some(changed);
        }
    }

    /// Runs on every rows reply; only an armed reload owes a notice, and only when rows changed.
    pub fn landed(&mut self, total: usize) -> Option<usize> {
        let from = self.from.take()?;
        let changed = self.changed.take().unwrap_or_else(|| total.abs_diff(from));
        (changed > 0).then_some(changed)
    }

    pub fn armed(&self) -> bool {
        self.from.is_some()
    }
}

/// Why Enter opened nothing for a marked selection; each is said in foreground with its full stop, since nothing failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenRefusal {
    FolderWithFiles,
    Archives,
    Programs,
    NotLoaded,
}

impl OpenRefusal {
    pub fn key(self) -> &'static str {
        match self {
            Self::FolderWithFiles => "feedback.refusal.folder_with_files",
            Self::Archives => "feedback.refusal.archives",
            Self::Programs => "feedback.refusal.programs",
            Self::NotLoaded => "feedback.refusal.not_loaded",
        }
    }
}
