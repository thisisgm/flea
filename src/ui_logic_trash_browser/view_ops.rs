//! The Trash view's actions: marks, restore, the delete review, the dd pair, the menu and its keys.
use super::selection::Extend;
use super::view::{Anchor, Keys, Press, Review, Signal, View};
use super::words::{self, MenuTarget, Route};
use crate::i18n::I18n;

impl View {

    pub fn select_all(&mut self) {
        if self.operation_active || self.confirming || self.pending_op == "select" || self.total == 0 {
            return;
        }
        self.selection.clear();
        self.send("select", Self::browse());
    }

    pub fn choose(&mut self, index: isize, extend: Extend) {
        self.arm.clear();
        if self.total == 0 || self.confirming || self.operation_active {
            return;
        }
        if self.pending_op == "select" {
            self.send("cancel", Self::browse().flag("clearSelection", true));
        }
        self.cursor = index.clamp(0, self.total as isize - 1) as usize;
        self.contain(self.cursor);
        let Some(row) = self.row_at(self.cursor).cloned() else {
            self.pending_selection = Some((self.cursor, extend));
            return self.ensure_window();
        };
        self.pending_selection = None;
        self.selection.choose(&row, extend, &self.rows);
    }

    pub fn extend_selection(&mut self, direction: isize) {
        let previous = self.cursor as isize;
        self.choose(previous, Extend::Add);
        self.choose(previous + direction, Extend::Add);
    }

    pub fn restore(&mut self, all: bool) {
        if self.busy() || if all { self.total == 0 } else { self.selection.is_empty() } {
            return;
        }
        self.confirming = false;
        let uris = self.selection.uris();
        let token = if !all && self.selection.all { self.selection.token } else { 0 };
        let excluded = if all { Vec::new() } else { self.selection.excluded() };
        let identities = self.selection.identities_of(&uris);
        let line = Self::browse().flag("all", all).number("selectionToken", token).texts("uris", &uris)
            .texts("exclude", &excluded).texts("identities", &identities);
        self.send("restore", line);
    }

    pub fn prepare(&mut self, all: bool) {
        if self.busy() || if all { self.total == 0 } else { self.selection.is_empty() } {
            return;
        }
        self.prepare_review(all);
    }

    /// Send the deletion review request for the entire Trash or the current selection.
    pub(super) fn prepare_review(&mut self, all: bool) {
        self.confirming = true;
        self.refresh_pending = false;
        let uris = self.selection.uris();
        self.review = Review {
            all,
            selection_all: all,
            selection_token: if !all && self.selection.all { self.selection.token } else { 0 },
            identities: self.selection.identities_of(&uris),
            excluded: if all { Vec::new() } else { self.selection.excluded() },
            uris,
            token: 0,
        };
        let r = &self.review;
        let line = Self::browse().flag("all", r.selection_all).flag("emptyTrash", all).texts("uris", &r.uris)
            .texts("exclude", &r.excluded).texts("identities", &r.identities).number("selectionToken", r.selection_token);
        self.send("prepare", line);
    }

    /// The view's own dd: the pair reviews permanent deletion of the marked rows.
    pub fn arm_delete(&mut self, now_ms: i64, i18n: &I18n, keys: Keys) {
        if self.selection.is_empty() || self.busy() || self.confirming {
            return self.arm.clear();
        }
        if self.arm.press(now_ms) {
            self.prepare(false);
        } else {
            self.signals.push(Signal::Arm(words::review_prompt(i18n, keys.arm, keys.alone)));
        }
    }

    /// The card's answer: the destructive button sends its token, anything else cancels.
    pub fn confirmed(&mut self, token: Option<u64>) {
        self.confirmation = None;
        self.confirming = false;
        match token {
            Some(token) => self.send("delete", Self::browse().number("token", token)),
            None => self.send("cancel", Self::browse()),
        }
    }

    /// The check timer: an open card asks whether its reviewed items are unchanged.
    pub fn check(&mut self) {
        let Some(token) = self.confirmation.as_ref().map(|c| c.token) else { return };
        if !self.busy() {
            self.send("check", Self::browse().number("token", token));
        }
    }

    /// m: the cursor row's own menu, the marked rows' menu, or the whole-Trash menu when nothing lists.
    pub fn open_menu(&mut self) {
        let target = match self.row_at(self.cursor) {
            Some(row) => words::menu_target(self.total, self.selected_count(), self.is_selected(&row.uri)),
            None => MenuTarget::Strip,
        };
        if target == MenuTarget::Cursor {
            if self.selection.is_empty() {
                self.choose(self.cursor as isize, Extend::No);
            }
            self.contain(self.cursor);
            return self.signals.push(Signal::Context { at: Anchor::Row(self.cursor), selection: true });
        }
        let selection = !self.selection.is_empty();
        self.signals.push(Signal::Context { at: Anchor::Top, selection });
    }

    /// Keys.onPressed: Trash owns this focus context, so every key is taken; an open card holds the focus itself, a review in flight does not.
    pub fn press(&mut self, press: Press, repeat: bool, now_ms: i64, i18n: &I18n, keys: Keys) {
        if self.confirmation.is_some() {
            return;
        }
        let action = match press {
            Press::Action(action) => action,
            Press::Backspace => "escape",
            Press::F5 => "reload",
        };
        if action == "trashArm" && repeat {
            return;
        }
        if action != "trashArm" {
            self.arm.clear();
        }
        let page = self.page().min(super::view::WINDOW_CAP).saturating_sub(2).max(1) as isize;
        let cursor = self.cursor as isize;
        match action {
            "escape" => self.close(),
            "cursorDown" => self.choose(cursor + 1, Extend::No),
            "cursorUp" => self.choose(cursor - 1, Extend::No),
            "cursorFirst" => self.choose(0, Extend::No),
            "cursorLast" => self.choose(self.total as isize - 1, Extend::No),
            "pageDown" => self.choose(cursor + page, Extend::No),
            "pageUp" => self.choose(cursor - page, Extend::No),
            "toggleSelect" => self.choose(cursor, Extend::Toggle),
            "extendDown" => self.extend_selection(1),
            "extendUp" => self.extend_selection(-1),
            "selectAll" => self.select_all(),
            "trashArm" => self.arm_delete(now_ms, i18n, keys),
            "deletePermanently" | "trash" => self.prepare(false),
            "focusNext" => self.signals.push(Signal::FocusRail),
            _ if action == "quit" || words::route(action) != Route::Trash => self.signals.push(Signal::Action(action.into())),
            "reload" => self.refresh(),
            "menu" => self.open_menu(),
            _ => {}
        }
    }
}
