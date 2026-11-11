//! Transfer completion receipts, whole for the card and split from their undo advice for the footer.
use super::{transfer_body, with_undo, Outcome};
use crate::i18n::I18n;
use crate::ui_logic_file_ops::{collide, transfer::Transfer, TransferDone};

// The done line off the transferdone line itself, so a refused merge never mislabels the next copy.
pub fn done_line(catalog: &I18n, preset: &str, t: &Transfer, done: &TransferDone) -> String {
    match done_receipt(catalog, t, done) {
        (line, true) => with_undo(catalog, preset, line),
        (line, false) => line,
    }
}

/// The receipt without its undo advice, and whether anything landed that the undo can take back.
pub fn done_receipt(catalog: &I18n, t: &Transfer, done: &TransferDone) -> (String, bool) {
    if done.into.is_empty() {
        let o = Outcome {
            ok: done.ok,
            failed: done.failed,
            skipped: done.skipped,
            cancelled: done.cancelled,
            note: &done.note,
        };
        return (transfer_body(catalog, t, o), done.ok > 0);
    }
    let m = collide::Merged {
        merged: done.merged,
        kept_both: done.kept_both,
        into: &done.into,
        failed: done.failed,
        skipped: done.skipped,
        cancelled: done.cancelled,
        note: &done.note,
    };
    (collide::merged_body(catalog, m), done.merged > 0)
}
