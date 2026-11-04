// The dd pair, the permanent-delete card and Empty Trash, as decisions with an injected clock.
use super::{count, duration, lines::items as items_word, n, size, Part};
use crate::i18n::I18n;
use crate::keys::presentation::{key_for, key_of};

// Long enough to be a deliberate pair, short enough that a stray d minutes later cannot finish it.
pub const ARM_MS: u64 = 1500;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Arm {
    pub stamp: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Armed,
    Act,
}

impl Arm {
    // The stamp is read before it is written, which is what makes the second press the pair.
    pub fn press(&mut self, now: u64) -> Press {
        if self.live(now) {
            self.stamp = 0;
            return Press::Act;
        }
        self.stamp = now.max(1);
        Press::Armed
    }

    // Any other key, a new listing or the clock ends it.
    pub fn disarm(&mut self) {
        self.stamp = 0;
    }

    pub fn live(&self, now: u64) -> bool {
        self.stamp > 0 && now.saturating_sub(self.stamp) < ARM_MS
    }

    // What the arm has left by its own stamp, so a timer that fires early never ends it while d still acts.
    pub fn left(&self, now: u64) -> u64 {
        if self.stamp == 0 {
            0
        } else {
            ARM_MS.saturating_sub(now.saturating_sub(self.stamp))
        }
    }
}

// The arming key and the key that acts alone, both from the running preset.
pub fn trash_prompt(catalog: &I18n, preset: &str, rows: usize) -> String {
    let arming = key_for(preset, "trash");
    let alone = key_of(preset, "trash", "none");
    let many = rows > 1;
    let key = match (alone.is_empty(), many) {
        (false, false) => "jobs.arm.trash",
        (false, true) => "jobs.arm.trash_many",
        (true, false) => "jobs.arm.trash_alone",
        (true, true) => "jobs.arm.trash_many_alone",
    };
    catalog.tr_args(key, &[&arming, &alone])
}

pub fn review_prompt(catalog: &I18n, preset: &str) -> String {
    let alone = key_of(preset, "trash", "none");
    let key = if alone.is_empty() {
        "jobs.arm.review_alone"
    } else {
        "jobs.arm.review"
    };
    catalog.tr_args(key, &[&key_for(preset, "trash"), &alone])
}

// A live arm's prompt, else the oldest unacknowledged error, else the notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transient {
    pub text: String,
    pub error: bool,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub text: String,
    pub detail: String,
    pub place: String,
}

pub fn transient(errors: &[Refusal], notice: &str, prompt: &str, live: bool) -> Transient {
    match (live, errors.first()) {
        (true, _) => Transient {
            text: prompt.into(),
            error: false,
            detail: String::new(),
        },
        (false, Some(e)) => Transient {
            text: e.text.clone(),
            error: true,
            detail: e.detail.clone(),
        },
        _ => Transient {
            text: notice.into(),
            error: false,
            detail: String::new(),
        },
    }
}

pub fn no_trash_line(catalog: &I18n, preset: &str) -> String {
    catalog.tr("jobs.trash.none")
        + &super::hint(catalog, preset, "jobs.hint.deletes", "deletePermanently")
}

// The no-Trash refusal names its place and replaces its own copy there; other errors name none.
pub fn with_error(
    errors: &[Refusal],
    text: &str,
    detail: &str,
    place: &str,
    no_trash: &str,
) -> Vec<Refusal> {
    let entry = Refusal {
        text: text.into(),
        detail: detail.into(),
        place: if text == no_trash {
            place.into()
        } else {
            String::new()
        },
    };
    let mut next = errors.to_vec();
    match next
        .iter()
        .position(|e| !entry.place.is_empty() && e.text == entry.text && e.place == entry.place)
    {
        Some(at) => next[at] = entry,
        None => next.push(entry),
    }
    next
}

// Leaving a place drops its refusals; other errors wait for dismissal.
pub fn errors_at(errors: &[Refusal], place: &str) -> Vec<Refusal> {
    errors
        .iter()
        .filter(|e| e.place.is_empty() || e.place == place)
        .cloned()
        .collect()
}

// Where Delete goes: Trash where there is one, the permanent card where the place is writable without one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Trash,
    ConfirmPermanent,
    Refuse,
}

// A read-only place is not trashable either, so d there refuses as v0.3.12's trashRefused does.
pub fn delete_route(has_trash: bool, writable: bool) -> Route {
    match (has_trash, writable) {
        (_, false) => Route::Refuse,
        (true, true) => Route::Trash,
        (false, true) => Route::ConfirmPermanent,
    }
}

// The window-level keys the Trash view borrows: direct skips the pane, act goes through it.
pub fn view_route(action: &str) -> &'static str {
    if action == "keymapSheet" || action == "pathBar" || action.starts_with("textSize") {
        "direct"
    } else if action == "settings" || action == "sidebar" || action.starts_with("tab") {
        "act"
    } else {
        "trash"
    }
}

// Where m opens in the Trash view: the cursor row, the marked rows, or the whole Trash.
pub fn menu_target(total: u64, marked: u64, cursor_marked: bool) -> &'static str {
    if total == 0 {
        "strip"
    } else if marked == 0 || marked == 1 && cursor_marked {
        "cursor"
    } else {
        "marked"
    }
}

// The review snapshot the card is drawn from: the whole filled card or nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Review {
    pub count: Option<u64>,
    pub bytes: u64,
    pub all: bool,
    pub token: String,
    pub stale: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Items,
    TrashItems,
}

pub fn confirm_title(catalog: &I18n, review: &Review) -> String {
    match review.count {
        None => String::new(),
        Some(_) if review.all => catalog.tr("jobs.delete.empty_title"),
        Some(c) => catalog.trn("jobs.delete.title", n(c)),
    }
}

pub fn scope_line(catalog: &I18n, scope: Scope, rows: u64) -> String {
    catalog.trn(
        match scope {
            Scope::Items => "jobs.delete.body_items",
            Scope::TrashItems => "jobs.delete.body_trash",
        },
        n(rows),
    )
}

pub fn all_line(catalog: &I18n, rows: u64, size_text: &str) -> String {
    catalog.trn_args(
        "jobs.delete.body_all",
        n(rows),
        &[&items_word(catalog, rows), size_text],
    )
}

pub fn confirm_body(catalog: &I18n, review: &Review, scope: Scope) -> String {
    match review.count {
        None => String::new(),
        Some(c) if review.all => all_line(catalog, c, &size(catalog, review.bytes)),
        Some(c) => scope_line(catalog, scope, c),
    }
}

// Delete acts only on a filled review the backend has not yet called stale; Cancel always acts.
pub fn delete_enabled(review: &Review, running: bool) -> bool {
    review.count.is_some_and(|c| c > 0) && !review.token.is_empty() && !review.stale && !running
}

// The permanent card's buttons, left to right: Cancel is the default and keeps its accent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Cancel,
    Delete,
}

pub const BUTTONS: [Button; 2] = [Button::Cancel, Button::Delete];
pub const PRIMARY: Button = Button::Cancel;

impl Button {
    // The destructive button names the whole Trash when the review is the whole Trash.
    pub fn label_key(self, all: bool) -> &'static str {
        match self {
            Button::Cancel => "jobs.button.cancel",
            Button::Delete if all => "jobs.button.empty_trash",
            Button::Delete => "jobs.button.delete",
        }
    }
}

// Empty Trash's own progress: the walk that checks, then the walk that deletes, by item count.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Emptying {
    pub id: u64,
    pub running: bool,
    pub phase: String,
    pub name: String,
    pub item_size: u64,
    pub done: u64,
    pub total: u64,
    pub checked: u64,
    pub first_ms: u64,
    pub first_done: u64,
    pub last_ms: u64,
}

impl Emptying {
    pub fn started(id: u64, total: u64, now: u64) -> Self {
        Self {
            id,
            running: true,
            total,
            first_ms: now,
            last_ms: now,
            ..Self::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn sampled(
        &mut self,
        id: u64,
        phase: &str,
        name: &str,
        item_size: u64,
        done: u64,
        total: u64,
        checked: u64,
        now: u64,
    ) -> bool {
        if id != self.id || !self.running {
            return false;
        }
        if phase != self.phase {
            self.first_ms = now;
            self.first_done = done;
        }
        self.phase = phase.into();
        self.name = name.into();
        self.item_size = item_size;
        self.done = done;
        self.checked = checked;
        if total > 0 {
            self.total = total;
        }
        self.last_ms = now;
        true
    }

    // Deleted items per second over this phase so far; no estimate until a second has passed.
    pub fn left(&self) -> Option<u64> {
        let spent = self.last_ms.saturating_sub(self.first_ms);
        let gone = self.done.saturating_sub(self.first_done);
        if self.phase != "deleting" || spent < 1000 || gone == 0 {
            return None;
        }
        Some(self.total.saturating_sub(self.done) * spent / gone / 1000)
    }

    pub fn fraction(&self) -> f64 {
        let (at, of) = if self.phase == "checking" {
            (self.checked, self.total)
        } else {
            (self.done, self.total)
        };
        if of == 0 {
            0.0
        } else {
            super::transfer::unit(at as f64 / of as f64)
        }
    }

    pub fn head(&self, catalog: &I18n) -> String {
        catalog.tr("jobs.empty.head")
    }

    pub fn file_line(&self, catalog: &I18n) -> String {
        if self.name.is_empty() {
            String::new()
        } else if self.item_size > 0 {
            catalog.tr_args(
                "jobs.transfer.file",
                &[&self.name, &size(catalog, self.item_size)],
            )
        } else {
            self.name.clone()
        }
    }

    pub fn count_parts(&self, catalog: &I18n) -> Vec<Part> {
        let (done, total) = (count(catalog, self.done), count(catalog, self.total));
        if self.phase == "checking" {
            return super::parts(
                &catalog.tr("jobs.empty.checked"),
                &[&count(catalog, self.checked), &total],
            );
        }
        match self.left() {
            Some(left) => super::parts(
                &catalog.tr("jobs.empty.deleted_left"),
                &[&done, &total, &duration(left)],
            ),
            None => super::parts(&catalog.tr("jobs.empty.deleted"), &[&done, &total]),
        }
    }

    // Exactly one done ends it; a done for another id is not this one's.
    pub fn finish(&mut self, id: u64) -> bool {
        if id != self.id {
            return false;
        }
        self.running = false;
        true
    }
}

// While Empty Trash runs its own button sits at the disabled opacity and a second press is refused.
pub fn empty_button_enabled(total: u64, running: bool) -> bool {
    total > 0 && !running
}
