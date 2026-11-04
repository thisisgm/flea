//! The clipboard's words: every user-visible sentence under the clipboard prefix, kept byte for byte from ui/js/Clipboard.js, Ops.js and Menu.js; the paste hint is composed by the status slot (G1.13), and the stamped image name is a filename, so it stays a literal in paste.
use crate::i18n::I18n;
use super::Said;

// Sample input: count 1 reads "Copied 1 item"; both keys are the shared plural catalog rows the TUI also reads.
pub const COPIED: &str = "clipboard.copied";
pub const CUT: &str = "clipboard.cut";
pub const SET_FAILED: &str = "clipboard.set_failed";
pub const CLEAR_FAILED: &str = "clipboard.clear_failed";
pub const DEST_CHANGED: &str = "clipboard.dest_changed";
pub const READING: &str = "clipboard.reading";
pub const RESOLVING: &str = "clipboard.resolving";
pub const STALE: &str = "clipboard.stale";
pub const RECENT_PASTE: &str = "clipboard.recent_paste";

// Sample input: 3 items; copy reads "Copied 3 items", cut reads "Cut 3 items". The line is hint-marked, so the status slot appends the paste key as Ops.copied wraps Status.withPaste.
pub fn landed(i18n: &I18n, moving: bool, count: usize) -> Said {
    Said {
        text: i18n.trn(if moving { CUT } else { COPIED }, count as i64),
        error: false,
        paste_hint: true,
    }
}

// Sample input: none; the Recent view has no folder to paste into. Info role, as the oracle's root.message(..., false) at Focus.js:217,235,237.
pub fn recent_paste(i18n: &I18n) -> Said {
    Said { text: i18n.tr(RECENT_PASTE), error: false, paste_hint: false }
}

// Sample input: "no compositor" reads "Copied in this window only: no compositor".
pub fn set_failed(i18n: &I18n, error: &str) -> String {
    i18n.tr_args(SET_FAILED, &[error])
}

// Sample input: "no compositor" reads "Could not clear the system clipboard: no compositor".
pub fn clear_failed(i18n: &I18n, error: &str) -> String {
    i18n.tr_args(CLEAR_FAILED, &[error])
}

pub fn dest_changed(i18n: &I18n) -> String {
    i18n.tr(DEST_CHANGED)
}

pub fn reading(i18n: &I18n) -> String {
    i18n.tr(READING)
}

pub fn resolving(i18n: &I18n) -> String {
    i18n.tr(RESOLVING)
}

pub fn stale(i18n: &I18n) -> String {
    i18n.tr(STALE)
}

// A request already sent and held when its listing moved: the backend's stale refusal Errors.js says, in error ink.
pub fn landed_stale(i18n: &I18n) -> Said {
    Said { text: crate::ui_logic_fb1::errors::sentence("stale", "", "").render(i18n), error: true, paste_hint: false }
}
