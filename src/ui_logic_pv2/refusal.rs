//! The refusal classes of the code, HTML, font and archive previews and the fixed sentence each one projects.
//!
//! A backend reply names a state, never a sentence the preview shows: the converter's or tool's own text stays
//! diagnostic data, and each class reads one catalog key taken from the frozen v0.3.12 panes.

/// Why a preview draws no content of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Over the byte gate by the listing's size or by the backend's own count (CodeRefusal.qml, PreviewHtml.qml).
    TooLarge,
    /// A NUL in the file's first bytes, so its lines are not text (CodeRefusal.qml).
    Binary,
    /// The file or the helper's answer could not be read (CodeRefusal.qml, PreviewHtml.qml).
    Unreadable,
    /// The font tool refused the file (FontPane.qml).
    FontUnsupported,
    /// The archive's index could not be listed (Preview.qml archiveFailed).
    ArchiveUnreadable,
}

impl Refusal {
    /// The status sentence Quick Look reports; a binary file reports none, since its frame is its whole answer.
    pub fn status_key(self) -> Option<&'static str> {
        match self {
            Refusal::TooLarge => Some("document.too_large"),
            Refusal::Binary => None,
            Refusal::Unreadable => Some("document.unreadable"),
            Refusal::FontUnsupported => Some("document.font_unsupported"),
            Refusal::ArchiveUnreadable => Some("document.archive_unreadable"),
        }
    }

    /// The caption a code frame draws under the kind's mark; the other classes draw their sentence alone.
    pub fn frame_key(self) -> Option<&'static str> {
        match self {
            Refusal::TooLarge => Some("document.too_large_frame"),
            Refusal::Binary => Some("document.binary_frame"),
            _ => None,
        }
    }

    /// The sentence drawn alone in the frame, for the classes with no mark and caption.
    pub fn sentence_key(self) -> Option<&'static str> {
        self.frame_key().map_or(self.status_key(), |_| None)
    }
}

/// The refusal a code or HTML Source answer state carries; styled, plain and an answer still out carry none.
pub fn from_code_state(state: &str) -> Option<Refusal> {
    match state {
        "binary" => Some(Refusal::Binary),
        "large" => Some(Refusal::TooLarge),
        "failed" => Some(Refusal::Unreadable),
        _ => None,
    }
}

#[cfg(test)]
#[path = "refusal_tests.rs"]
mod tests;
