//! Picker actions reuse the generated key table and the shared list/grid cursor rules.
use super::Picker;
use crate::window::input::Dispatch;
use gpui::{Context, KeyDownEvent, Window};

impl Picker {
    pub(super) fn listing_action(
        &mut self,
        action: &str,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.chrome
            .listing_action
            .is_some_and(|run| run(self, action, event, cx))
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
            Default::default(),
        );
        let action = match dispatch {
            Dispatch::Action(action) => action,
            Dispatch::Armed => return true,
            _ => "",
        };
        // Shift+Up/Down extends a range only where one is permitted; elsewhere it moves the cursor as plain Up/Down.
        let action = match action {
            "extendDown" if !self.selection_permitted() => "cursorDown",
            "extendUp" if !self.selection_permitted() => "cursorUp",
            other => other,
        };
        let key = event.keystroke.key.as_str();
        if self.browse.listing.read(cx).view_mode() == "grid"
            && !text_focus
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
        } else if action == "selectAll" && !text_focus {
            self.select_all(cx);
        } else if key == "space" && !text_focus {
            self.toggle_mark(self.listing.cursor, cx);
        } else if self.browse.listing.read(cx).view_mode() == "grid" && !text_focus {
            return self.grid_key(action, key, chord.shift && self.selection_permitted(), cx)
                || self.listing_action(action, event, cx);
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
            return self.listing_action(action, event, cx);
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
