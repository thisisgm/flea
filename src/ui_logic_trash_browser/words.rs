//! Trash.js: the dd pair, key routing, the m menu target and every Trash sentence, each one whole catalog string.
use super::dates;
use crate::i18n::I18n;

/// How long the first d stays armed: a deliberate pair, never one finished by a d typed minutes later.
pub const ARM_MS: i64 = 1500;

/// The dd stamp; any other key clears it, so an arm never survives the key after it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Arm {
    at_ms: Option<i64>,
}

impl Arm {
    /// True when this press completes a pair inside the window; otherwise it arms afresh.
    pub fn press(&mut self, now_ms: i64) -> bool {
        let paired = self.at_ms.is_some_and(|at| now_ms - at < ARM_MS);
        self.at_ms = if paired { None } else { Some(now_ms) };
        paired
    }
    pub fn clear(&mut self) {
        self.at_ms = None;
    }
    pub fn armed(&self) -> bool {
        self.at_ms.is_some()
    }
}

/// Where a window-level key goes from the Trash view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Straight to the window handler, skipping the pane.
    Direct,
    /// Through the pane's own action dispatch.
    Act,
    /// Kept by Trash; ordinary listing keys never reach the covered pane.
    Trash,
}

pub fn route(action: &str) -> Route {
    if matches!(action, "keymapSheet" | "pathBar" | "undo" | "redo")
        || action.starts_with("textSize")
    {
        Route::Direct
    } else if action == "settings" || action == "sidebar" || action.starts_with("tab") {
        Route::Act
    } else {
        Route::Trash
    }
}

/// Which menu m opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuTarget {
    Strip,
    Cursor,
    Marked,
}

pub fn menu_target(total: usize, marked: usize, cursor_marked: bool) -> MenuTarget {
    if total == 0 {
        MenuTarget::Strip
    } else if marked == 0 || (marked == 1 && cursor_marked) {
        MenuTarget::Cursor
    } else {
        MenuTarget::Marked
    }
}

/// The running preset's keys for the prompt: the arming key, and the key that trashes alone, empty when none is bound.
pub fn prompt_keys(preset: &str) -> (String, String) {
    (
        crate::keys::presentation::key_for(preset, "trash"),
        crate::keys::presentation::key_of(preset, "trash", "none"),
    )
}

/// Ops.targetIndices for the pair: the marked rows win over the cursor row.
pub fn targets(selected: &[usize], cursor: usize) -> Vec<usize> {
    if selected.is_empty() {
        vec![cursor]
    } else {
        selected.to_vec()
    }
}

/// The dd prompt: the arming key, and the key that acts alone when the preset binds one; the count picks it or them.
pub fn trash_prompt(i18n: &I18n, count: usize, arm_key: &str, alone_key: &str) -> String {
    let many = count > 1;
    match (alone_key.is_empty(), many) {
        (false, false) => i18n.tr_args("trash.arm.one", &[arm_key, alone_key]),
        (false, true) => i18n.tr_args("trash.arm.many", &[arm_key, alone_key]),
        (true, false) => i18n.tr_args("trash.arm.one_alone", &[arm_key]),
        (true, true) => i18n.tr_args("trash.arm.many_alone", &[arm_key]),
    }
}

/// The Trash view's own dd, which reviews permanent deletion.
pub fn review_prompt(i18n: &I18n, arm_key: &str, alone_key: &str) -> String {
    if alone_key.is_empty() {
        i18n.tr_args("trash.arm.review_alone", &[arm_key])
    } else {
        i18n.tr_args("trash.arm.review", &[arm_key, alone_key])
    }
}

/// The delete card's snapshot: unfilled until the backend counts it, and an unfilled card says no word at all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub count: Option<i64>,
    pub all: bool,
    pub bytes: u64,
    pub token: u64,
}

/// Whose items a scoped delete names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Trash,
    Items,
}

pub fn confirm_title(i18n: &I18n, snapshot: &Snapshot) -> String {
    match snapshot.count {
        None => String::new(),
        Some(_) if snapshot.all => i18n.tr("trash.confirm.empty_title"),
        Some(count) => i18n.trn("trash.confirm.delete_title", count),
    }
}

pub fn confirm_body(i18n: &I18n, snapshot: &Snapshot, scope: Scope) -> String {
    let Some(count) = snapshot.count else {
        return String::new();
    };
    if snapshot.all {
        let size = i18n.locale().format_size(snapshot.bytes);
        return i18n.trn_args("trash.confirm.empty_body", count, &[&size]);
    }
    match scope {
        Scope::Trash => i18n.trn("trash.confirm.trash_body", count),
        Scope::Items => i18n.trn("trash.confirm.items_body", count),
    }
}

/// The strip's count, then its size once the summary answers, "≥" while the walk ran out of time.
pub fn strip_count(i18n: &I18n, total: usize, bytes: Option<(u64, bool)>) -> String {
    let count = i18n.trn("items.count", total as i64);
    match bytes {
        None => count,
        Some((bytes, partial)) => {
            let size = i18n.locale().format_size(bytes);
            let key = if partial {
                "trash.strip.size_partial"
            } else {
                "trash.strip.size"
            };
            i18n.tr_args(key, &[&count, &size])
        }
    }
}

/// "Restored 1 of 2 items", with the failures counted after it.
pub fn done_line(i18n: &I18n, restored: bool, done: usize, failed: usize) -> String {
    let key = if restored {
        "trash.done.restored"
    } else {
        "trash.done.deleted"
    };
    let done_text = i18n.locale().format_count(done as i64);
    let line = i18n.trn_args(key, (done + failed) as i64, &[&done_text]);
    if failed == 0 {
        return line;
    }
    i18n.tr_args(
        "trash.done.failed",
        &[&line, &i18n.locale().format_count(failed as i64)],
    )
}

/// The finished line, adding how many entries the listing left out that Empty Trash could not remove.
pub fn leftover_line(i18n: &I18n, line: String, leftover: usize) -> String {
    if leftover == 0 {
        return line;
    }
    i18n.trn_args("trash.done.leftover", leftover as i64, &[&line])
}

/// One failed item's line; the backend's reason is its own diagnostic, carried as an argument.
pub fn failure_line(i18n: &I18n, name: &str, error: &str) -> String {
    i18n.tr_args("trash.done.failure", &[name, error])
}

pub fn recovered(i18n: &I18n, count: i64) -> String {
    i18n.trn("trash.recovered", count)
}

/// A row's Original Location cell, as a person reads a folder that is not UTF-8.
pub fn location_text(i18n: &I18n, original: &str, home: &str) -> String {
    dates::location(&crate::bytename::display(original), home)
        .unwrap_or_else(|| i18n.tr("trash.unknown"))
}

/// A row's Deleted cell, the product's one date form.
pub fn deleted_text(i18n: &I18n, text: Option<&str>) -> String {
    dates::deleted(text)
        .and_then(|seconds| i18n.locale().format_date(seconds))
        .unwrap_or_else(|| i18n.tr("trash.unknown"))
}
