// The status lines operations end in, as whole translated sentences, and the small routes beside them.
use super::{count, hint, leaf, n, transfer::Transfer, undo_hint};
use crate::i18n::I18n;

mod link;
mod receipt;
pub use link::{linked_line, linked_receipt};
pub use receipt::{done_line, done_receipt};

pub fn items(catalog: &I18n, value: u64) -> String {
    catalog.trn("jobs.items", n(value))
}

pub fn with_undo(catalog: &I18n, preset: &str, line: String) -> String {
    line + &undo_hint(catalog, preset)
}

pub fn with_paste(catalog: &I18n, preset: &str, line: String) -> String {
    line + &hint(catalog, preset, "jobs.hint.paste", "paste")
}

fn note(catalog: &I18n, text: &str) -> String {
    catalog.tr_args("jobs.hint.note", &[text])
}

// The count tail every partial line shares: failed, skipped, then cancelled.
fn tail(catalog: &I18n, failed: u64, skipped: u64, cancelled: bool) -> String {
    let mut line = String::new();
    if failed > 0 {
        line += &catalog.tr_args("jobs.done.failed_note", &[&count(catalog, failed)]);
    }
    if skipped > 0 {
        line += &catalog.tr_args("jobs.done.skipped_note", &[&count(catalog, skipped)]);
    }
    if cancelled {
        line += &catalog.tr("jobs.done.cancelled_note");
    }
    line
}

// The status bar's one-line form of the card's headline, so both name the same sample.
pub fn progress_line(catalog: &I18n, t: &Transfer) -> String {
    let head = t.head(catalog);
    if t.name.is_empty() {
        head
    } else {
        catalog.tr_args("jobs.transfer.progress", &[&head, &t.name])
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Outcome<'a> {
    pub ok: u64,
    pub failed: u64,
    pub skipped: u64,
    pub cancelled: bool,
    pub note: &'a str,
}

// Only what landed is said: a verdict note with nothing copied stays unsaid, and so does the undo.
pub fn transfer_done(catalog: &I18n, preset: &str, t: &Transfer, o: Outcome<'_>) -> String {
    let line = transfer_body(catalog, t, o);
    if o.ok > 0 {
        with_undo(catalog, preset, line)
    } else {
        line
    }
}

/// The transfer receipt alone, for a footer that draws its undo advice as a separate target.
pub fn transfer_body(catalog: &I18n, t: &Transfer, o: Outcome<'_>) -> String {
    let partial = o.failed > 0 || o.skipped > 0 || o.cancelled;
    let mut line = if partial {
        catalog.tr_args(
            if t.moving {
                "jobs.done.moved_of"
            } else {
                "jobs.done.copied_of"
            },
            &[&count(catalog, o.ok), &count(catalog, t.n)],
        )
    } else {
        catalog.tr_args(
            if t.moving {
                "jobs.done.moved"
            } else {
                "jobs.done.copied"
            },
            &[&items(catalog, o.ok)],
        )
    };
    line += &tail(catalog, o.failed, o.skipped, o.cancelled);
    if o.ok > 0 && !o.note.is_empty() {
        line += &note(catalog, o.note);
    }
    line
}

// A fixed cause, a drive's name refusal among them, reads in the catalog's language.
pub fn transfer_failure(catalog: &I18n, moving: bool, name: &str, error: &str) -> String {
    let error = crate::ui_logic_status_feedback::errors::clause(error, catalog);
    catalog.tr_args(
        if moving {
            "jobs.done.move_failed"
        } else {
            "jobs.done.copy_failed"
        },
        &[name, &error],
    )
}

// Only an identity-checked locate reply supplies these matches.
pub fn retry_selection_line(catalog: &I18n, matches: &[&str]) -> String {
    match matches {
        [] => String::new(),
        [one] => catalog.tr_args("jobs.done.retry_selected", &[leaf(one)]),
        many => catalog.tr_args(
            "jobs.done.retry_selected",
            &[&items(catalog, many.len() as u64)],
        ),
    }
}

// A failure names its reason, so a hung mount or a denial reads as what happened.
pub fn trashed(catalog: &I18n, preset: &str, ok: u64, failed: u64, reason: &str) -> String {
    let reason = &crate::ui_logic_status_feedback::errors::clause(reason, catalog);
    if ok == 0 {
        return match (failed == 1, reason.is_empty()) {
            (true, true) => catalog.tr("jobs.trash.failed_one"),
            (true, false) => catalog.tr_args("jobs.trash.failed_one_why", &[reason]),
            (false, true) => catalog.tr_args("jobs.trash.failed_many", &[&items(catalog, failed)]),
            (false, false) => catalog.tr_args(
                "jobs.trash.failed_many_why",
                &[&items(catalog, failed), reason],
            ),
        };
    }
    let line = if failed == 0 {
        catalog.tr_args("jobs.trash.moved", &[&items(catalog, ok)])
    } else if reason.is_empty() {
        catalog.tr_args(
            "jobs.trash.moved_some",
            &[&items(catalog, ok), &count(catalog, failed)],
        )
    } else {
        catalog.tr_args(
            "jobs.trash.moved_some_why",
            &[&items(catalog, ok), &count(catalog, failed), reason],
        )
    };
    with_undo(catalog, preset, line)
}

const JOURNAL: [&str; 11] = [
    "copy",
    "move",
    "rename",
    "merge",
    "permissions",
    "duplicate",
    "link",
    "bulkrename",
    "newfile",
    "paste",
    "cut",
];

// The op an undone line carries is the backend's own word; a word it uses for itself is never shown.
pub fn undone(catalog: &I18n, op: &str) -> String {
    match op {
        "trash" | "settags" | "mkdir" => catalog.tr(&format!("jobs.undone.{op}")),
        _ if JOURNAL.contains(&op) => catalog.tr(&format!("jobs.undone.{op}")),
        _ => catalog.tr("jobs.undone.other"),
    }
}

pub fn redone(catalog: &I18n, op: &str) -> String {
    if JOURNAL.contains(&op) {
        catalog.tr(&format!("jobs.redone.{op}"))
    } else {
        catalog.tr("jobs.redone.other")
    }
}

pub fn created_line(catalog: &I18n, path: &str) -> String {
    catalog.tr_args("jobs.done.created", &[leaf(path)])
}

pub fn made(catalog: &I18n, preset: &str, path: &str) -> String {
    with_undo(catalog, preset, created_line(catalog, path))
}

pub fn copied(catalog: &I18n, preset: &str, value: u64, moving: bool) -> String {
    with_paste(
        catalog,
        preset,
        catalog.tr_args(
            if moving {
                "jobs.done.cut"
            } else {
                "jobs.done.clip_copied"
            },
            &[&items(catalog, value)],
        ),
    )
}

pub fn bulk_done_line(catalog: &I18n, preset: &str, ok: u64) -> String {
    with_undo(catalog, preset, catalog.trn("jobs.done.renamed", n(ok)))
}

// Two kinds of success, said apart: an extract whose index could not be read was not verified.
pub fn archive_done_line(catalog: &I18n, verified: bool) -> String {
    catalog.tr(if verified {
        "jobs.done.archive_written"
    } else {
        "jobs.done.extract_unverified"
    })
}

// "Deleted 1 of 1 item" or "Restored 0 of 2 items", the Trash view's and the menu delete's status.
pub fn done_of(catalog: &I18n, verb: &str, done: u64, total: u64) -> String {
    catalog.trn_args(
        &format!("jobs.done.{verb}_of"),
        n(total),
        &[&count(catalog, done)],
    )
}

pub fn deleted_line(
    catalog: &I18n,
    deleted: u64,
    total: u64,
    failed: u64,
    cancelled: bool,
) -> String {
    done_of(catalog, "deleted", deleted, total) + &tail(catalog, failed, 0, cancelled)
}

// A refused Taildrop dispatch names its subject and its cause; an accepted one says it is sending.
pub fn taildrop_refused(catalog: &I18n, reason: &str) -> String {
    if reason.is_empty() {
        catalog.tr("jobs.taildrop.gone")
    } else {
        catalog.tr_args("jobs.taildrop.reason", &[reason])
    }
}

pub fn taildrop_stale(catalog: &I18n) -> String {
    catalog.tr("jobs.taildrop.stale")
}

pub fn taildrop_sending(catalog: &I18n, path: &str, peer: &str) -> String {
    catalog.tr_args("jobs.taildrop.sending", &[leaf(path), peer])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taildrop {
    Stale,
    Ignore,
    Send,
}

// Only a backend-validated absolute path is sent, and never for a folder or a missing cursor row.
pub fn taildrop_route(path: Option<&str>, cursor_row: Option<bool>) -> Taildrop {
    match (path, cursor_row) {
        (Some(p), _) if !p.starts_with('/') => Taildrop::Stale,
        (None, _) => Taildrop::Stale,
        (_, Some(false)) => Taildrop::Send,
        _ => Taildrop::Ignore,
    }
}

// The three reasons a cursor is not a target, in the order Enter asks them.
pub fn no_target(catalog: &I18n, cursor: i64, row_loaded: bool) -> String {
    catalog.tr(if cursor < 0 {
        "jobs.target.none"
    } else if !row_loaded {
        "jobs.target.not_loaded"
    } else {
        "jobs.target.filtered"
    })
}

pub fn paths_busy(catalog: &I18n) -> String {
    catalog.tr("jobs.target.busy")
}

// Only rows inside the held window can be named as a path; everything wider goes as indices.
pub fn target_paths<'a>(held: u64, rows: &[&'a str], indices: &[u64]) -> Vec<&'a str> {
    indices
        .iter()
        .filter_map(|i| {
            i.checked_sub(held)
                .and_then(|at| rows.get(at as usize))
                .copied()
        })
        .collect()
}

// The selection when there is one, else the cursor while the filter draws it.
pub fn target_indices(selected: &[u64], cursor: i64, cursor_shown: bool) -> Vec<u64> {
    if !selected.is_empty() {
        let mut out = selected.to_vec();
        out.sort_unstable();
        return out;
    }
    match u64::try_from(cursor) {
        Ok(at) if cursor_shown => vec![at],
        _ => Vec::new(),
    }
}

// Who asked for a paths round trip; an unclaimed reply lands on the clipboard, as it always did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claim {
    Clipboard,
    Compress(String),
    CopyAs(String),
    Permissions,
    BulkRename,
    Drag,
}

pub fn claim_of(kind: Option<&str>, format: &str) -> Claim {
    match kind {
        Some("compress") => Claim::Compress(format.into()),
        Some("copyAs") => Claim::CopyAs(format.into()),
        Some("permissions") => Claim::Permissions,
        Some("bulkrename") => Claim::BulkRename,
        Some("drag") => Claim::Drag,
        _ => Claim::Clipboard,
    }
}

// One round trip at a time: a second asker is refused out loud rather than stealing the reply.
pub fn refuse_while_busy(paths_pending: bool, clip_pending: bool) -> bool {
    paths_pending || clip_pending
}

// A ready snapshot over the unchanged identity opens now, one in flight takes F2, a moved one is stale.
pub fn menu_rename_route(ready: bool, identity: &str, current: &str) -> &'static str {
    if identity.is_empty() || identity != current {
        "stale"
    } else if ready {
        "now"
    } else {
        "f2"
    }
}

// A live editor shows the refusal itself; with none the status bar says it and the edit closes.
pub fn refuse_rename_route(editor_live: bool, renaming: bool) -> &'static str {
    if renaming && editor_live {
        "editor"
    } else {
        "status"
    }
}

// A name cannot be empty, . or .., or contain a separator or a NUL.
pub fn rename_refusal(catalog: &I18n, name: &str) -> Option<String> {
    let bad =
        name.is_empty() || name == "." || name == ".." || name.contains('/') || name.contains('\0');
    bad.then(|| catalog.tr("jobs.rename.invalid"))
}

pub fn rename_changed(catalog: &I18n) -> String {
    catalog.tr("jobs.rename.changed")
}

// The destination sits beside the source, whatever directory the pane has reached since.
pub fn rename_destination(source: &str, name: &str) -> String {
    let cut = source.rfind('/').map_or(0, |at| at + 1);
    format!("{}{name}", &source[..cut])
}
