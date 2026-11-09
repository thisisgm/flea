//! Link completion receipts share their counts and notes with the legacy whole-line formatter.
use super::{items, note, tail, with_undo, Outcome};
use crate::i18n::I18n;

// Paste as links answers one line per request, and one undo removes every link it made.
pub fn linked_line(catalog: &I18n, preset: &str, outcome: Outcome<'_>) -> String {
    let line = linked_receipt(catalog, outcome);
    if outcome.ok > 0 {
        with_undo(catalog, preset, line)
    } else {
        line
    }
}

/// The receipt alone, for a footer that draws its undo advice as a separate target.
pub fn linked_receipt(catalog: &I18n, outcome: Outcome<'_>) -> String {
    let mut line = catalog.tr_args("jobs.done.linked", &[&items(catalog, outcome.ok)]);
    line += &tail(catalog, outcome.failed, outcome.skipped, false);
    if !outcome.note.is_empty() {
        line += &note(catalog, outcome.note);
    }
    line
}
