//! Picker actions reuse the generated key table and the shared list/grid cursor rules.
use super::Picker;
use crate::{navigation::folder_state::Order, window::input::Dispatch};
use flea::ui_logic_pk2::sort;
use gpui::{Context, KeyDownEvent, Window};

impl Picker {
    // PickerList.qml and PickerGrid.qml's shared rows: g g, G, s, S and the period.
    pub(super) fn listing_action(&mut self, action: &str, cx: &mut Context<Self>) -> bool {
        let current = || self.browse.listing.read(cx).listing().order();
        match action {
            "cursorDown" | "cursorUp" => {
                self.shown_step(if action == "cursorDown" { 1 } else { -1 }, cx);
            }
            "cursorFirst" | "cursorLast" => self.shown_edge(action == "cursorLast", cx),
            "pageDown" | "pageUp" => {
                let rows = self.browse.listing.read(cx).list_visible_rows() as isize;
                self.shown_step(if action == "pageDown" { rows } else { -rows }, cx);
            }
            "sortNext" => {
                let order = current();
                let next = sort::picker_next_order(&sort::PickerOrder {
                    key: order.key.into(),
                    desc: false,
                });
                if let Some(key) = sort::PICKER_ORDERS.into_iter().find(|key| *key == next.key) {
                    self.request_sort(Order { key, desc: false }, cx);
                }
            }
            "sortReverse" => {
                let order = current();
                self.request_sort(
                    Order {
                        key: order.key,
                        desc: !order.desc,
                    },
                    cx,
                );
            }
            // PickerList.qml is disabled while submitting or without a backend, so the period re-lists nothing then.
            "toggleHidden" if !self.backend_unavailable() && !self.accept_state().submitting() => {
                self.req.hidden = Some(!self.show_hidden());
                let path = self.listing.location.clone();
                self.open_folder(&path, cx);
            }
            _ => return false,
        }
        true
    }

    // PickerList.qml moveCursor walks the rows a standing filter shows, so g and G land on a shown row.
    fn shown_edge(&mut self, last: bool, cx: &mut Context<Self>) {
        let shown = self.browse.listing.read(cx).shown().map(<[usize]>::to_vec);
        let total = shown.as_ref().map_or(self.listing.total, Vec::len);
        if total == 0 {
            return;
        }
        let at = if last { total - 1 } else { 0 };
        let target = shown.map_or(at, |rows| rows[at]);
        self.end_range();
        self.move_cursor(target, cx);
    }

    // PickerList.qml moveCursor(±visibleRows): a page clamps to the first or last shown row.
    fn shown_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let shown = self.browse.listing.read(cx).shown().map(<[usize]>::to_vec);
        let total = shown.as_ref().map_or(self.listing.total, Vec::len);
        if total == 0 {
            return;
        }
        let from = shown.as_ref().map_or(self.listing.cursor, |rows| {
            rows.iter()
                .position(|row| *row == self.listing.cursor)
                .unwrap_or(0)
        });
        let at = flea::ui_logic_pk2::page_target(from, delta, total);
        let target = shown.map_or(at, |rows| rows[at]);
        self.end_range();
        self.move_cursor(target, cx);
    }

    pub(super) fn parity_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.prefer_character_input {
            return false;
        }
        if self.filter_key(event, window, cx) {
            return true;
        }
        if self.control_key(event, window, cx) {
            return true;
        }
        if self.find.owner.typing()
            || self
                .browse
                .filename
                .as_ref()
                .is_some_and(|field| field.read(cx).entry.marked.is_some())
        {
            return false;
        }
        let text_focus = self
            .browse
            .filename
            .as_ref()
            .is_some_and(|field| field.read(cx).focus.is_focused(window));
        let chord = event.keystroke.modifiers;
        // Backspace under any modifier is the name field's own delete while it holds the keyboard.
        if text_focus
            && (event.keystroke.key.as_str() == "backspace"
                || !(chord.control || chord.alt || chord.platform))
        {
            return false;
        }
        self.chrome.keyboard.context = flea::keys::Context::Listing;
        let dispatch = self.chrome.keyboard.resolve(
            &self.chrome.preset,
            event,
            window.modifiers(),
            window.layout_mirrored(),
            Default::default(),
        );
        let action = match dispatch {
            Dispatch::Action(action) => action,
            Dispatch::Armed => return true,
            _ => "",
        };
        // Parent is a listing chord (ui/js/Keymap.js:19, :78); the name field keeps Ctrl+Up and Alt+Up.
        if text_focus && action == "parent" {
            return false;
        }
        // Shift+Up/Down extends a range only where one is permitted; elsewhere it moves the cursor as plain Up/Down.
        let action = match action {
            "extendDown" if !self.selection_permitted() => "cursorDown",
            "extendUp" if !self.selection_permitted() => "cursorUp",
            other => other,
        };
        let key = crate::window::input::reading(
            event.keystroke.key.as_str(),
            window.layout_mirrored() && !text_focus,
        );
        if self.browse.listing.read(cx).view_mode() == "grid"
            && !text_focus
            && action != "historyBack"
            && !(chord.control || chord.alt || chord.platform || chord.shift)
            && matches!(key, "left" | "right" | "h" | "l")
        {
            return self.grid_key(action, key, false, cx);
        }
        if action == "viewList" {
            self.user_set_view("list", cx);
        } else if action == "viewGrid" {
            self.user_set_view("grid", cx);
        } else if action == "parent" || key == "backspace" {
            self.go_up(cx);
        } else if action == "historyBack" {
            self.go_back(cx);
        } else if action == "pathBar" {
            // The shared entry opens in key(), before a Grid l can move the cursor.
            return false;
        } else if action == "selectAll" && !text_focus {
            self.select_all(cx);
        } else if key == "space" && !text_focus {
            if !self.name_from_row(self.listing.cursor, cx) {
                self.toggle_mark(self.listing.cursor, cx);
            }
        } else if self.browse.listing.read(cx).view_mode() == "grid" && !text_focus {
            return self.grid_key(action, key, chord.shift && self.selection_permitted(), cx)
                || self.listing_action(action, cx);
        } else if matches!(action, "extendDown" | "extendUp") && !text_focus {
            let to = if action == "extendDown" {
                self.listing
                    .cursor
                    .saturating_add(1)
                    .min(self.listing.total.saturating_sub(1))
            } else {
                self.listing.cursor.saturating_sub(1)
            };
            self.extend_to(to, cx);
        } else {
            return self.listing_action(action, cx);
        }
        true
    }

    fn grid_key(&mut self, action: &str, key: &str, extend: bool, cx: &mut Context<Self>) -> bool {
        let view = self.browse.listing.read(cx);
        let grid = view.grid_view();
        let shown = view.shown();
        let total = shown.map_or(self.listing.total, <[usize]>::len);
        let from = shown.map_or(self.listing.cursor, |rows| {
            rows.iter()
                .position(|row| *row == self.listing.cursor)
                .unwrap_or(0)
        });
        let layout = grid.layout(crate::grid::Shown::All(total));
        let delta = match key {
            "left" | "h" => Some(-1),
            "right" | "l" => Some(1),
            "up" | "k" => Some(-(layout.columns as isize)),
            "down" | "j" => Some(layout.columns as isize),
            _ => None,
        };
        let target = if let Some(delta) = delta {
            flea::ui_logic_pk2::path::grid_target(from, delta, layout.columns, total)
        } else {
            match action {
                "pageDown" => flea::ui_logic_pk2::page_target(
                    from,
                    (layout.columns * layout.visible_rows()) as isize,
                    total,
                ),
                "pageUp" => flea::ui_logic_pk2::page_target(
                    from,
                    -((layout.columns * layout.visible_rows()) as isize),
                    total,
                ),
                "cursorFirst" => 0,
                "cursorLast" => total.saturating_sub(1),
                _ => return false,
            }
        };
        let Some(target) = shown.map_or(Some(target), |rows| rows.get(target).copied()) else {
            return true;
        };
        if extend {
            self.extend_to(target, cx);
        } else {
            self.end_range();
            self.move_cursor(target, cx);
        }
        true
    }
}

#[cfg(test)]
#[path = "picker_grid_remainder_tests.rs"]
mod picker_grid_remainder_tests;
