//! One Tab ring and keyboard activation for the chooser's visible, enabled controls.
use super::Picker;
use gpui::{Context, FocusHandle, KeyDownEvent, Window};

impl Picker {
    pub(super) fn read_control_focus(&mut self, window: &Window) {
        self.chrome.focused = [
            ("picker-rail", &self.rail.focus),
            ("picker-back", &self.chrome.back),
            ("picker-up", &self.chrome.up),
            ("picker-filter", &self.chrome.filter),
            ("picker-cancel", &self.chrome.cancel),
            ("picker-answer", &self.chrome.answer),
            ("picker-new-folder", &self.chrome.new_folder),
            ("picker-folder-cancel", &self.chrome.folder_cancel),
            ("picker-folder-create", &self.chrome.folder_create),
            ("picker-rail", &self.rail.focus),
        ]
        .into_iter()
        .find(|(_, focus)| focus.is_focused(window))
        .map(|(id, _)| id);
    }

    fn focus_stops(&self, cx: &gpui::App) -> Vec<FocusHandle> {
        if self.accept_state().submitting() || self.folder_busy() {
            return vec![self.chrome.cancel.clone()];
        }
        let mut stops = Vec::new();
        if !self.chrome.history.is_empty() {
            stops.push(self.chrome.back.clone());
        }
        if self.can_go_up() {
            stops.push(self.chrome.up.clone());
        }
        if let Some(rail) = &self.chrome.rail_focus {
            stops.push(rail.clone());
        }
        stops.push(self.focus.clone());
        if flea::ui_logic_pk1::saving(&self.req) {
            stops.push(self.chrome.new_folder.clone());
            if let Some(filename) = &self.browse.filename {
                stops.push(filename.read(cx).focus.clone());
            }
        }
        if !self.req.filters.is_empty() {
            stops.push(self.chrome.filter.clone());
        }
        stops.push(self.chrome.cancel.clone());
        if self.can_accept() {
            stops.push(self.chrome.answer.clone());
        }
        stops
    }

    pub(super) fn step_focus(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let stops = self.focus_stops(cx);
        let at = stops.iter().position(|focus| focus.is_focused(window));
        let next = at.map_or(0, |at| {
            if back {
                (at + stops.len() - 1) % stops.len()
            } else {
                (at + 1) % stops.len()
            }
        });
        window.focus(&stops[next], cx);
        cx.notify();
    }

    pub(super) fn control_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.find.owner.typing()
            || self
                .browse
                .filename
                .as_ref()
                .is_some_and(|field| field.read(cx).entry.marked.is_some())
        {
            return false;
        }
        let key = event.keystroke.key.as_str();
        if key == "tab" {
            self.step_focus(event.keystroke.modifiers.shift, window, cx);
            return true;
        }
        let activates = matches!(key, "enter" | "space");
        if self.chrome.filter.is_focused(window) && (activates || key == "down") {
            self.open_filter(window, cx);
            return true;
        }
        if !activates {
            return false;
        }
        if self.chrome.cancel.is_focused(window) {
            self.cancel(cx);
        } else if self.chrome.answer.is_focused(window) {
            if self.accept_state().collision_review() {
                self.replace(cx);
            } else {
                self.accept(cx);
            }
        } else if self.chrome.back.is_focused(window) {
            self.go_back(cx);
        } else if self.chrome.up.is_focused(window) {
            self.go_up(cx);
        } else if self.chrome.new_folder.is_focused(window) {
            self.open_new_folder(window, cx);
        } else {
            return false;
        }
        true
    }

    // Focus and stem selection are applied with the window: a name that arrived, a review that asked, the default field.
    pub(super) fn place_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.browse.window = Some(window.window_handle());
        if self.place_folder_focus(window, cx) {
            return;
        }
        // A closed location field keeps no keyboard: Escape hands it back, so ":" and Ctrl+L reach the chooser again (EY-543).
        let path = self.browse.path_view(cx);
        if !path.read(cx).editing() && path.read(cx).focus.is_focused(window) {
            self.rail.hand_off = true;
        }
        // PickerWindow.qml focusDefault: Filename in a save, the files otherwise.
        if std::mem::take(&mut self.rail.hand_off) {
            if self.browse.filename.is_some() {
                self.browse.focus_filename = true;
            } else {
                window.focus(&self.focus, cx);
            }
        }
        let Some(filename) = self.browse.filename.clone() else {
            return;
        };
        let review = self.stem_requests != self.browse.stems_seen;
        if !self.browse.focus_filename && !review {
            return;
        }
        self.browse.focus_filename = false;
        self.browse.stems_seen = self.stem_requests;
        let focus = filename.update(cx, |field, cx| {
            if review {
                field.select_stem();
                cx.notify();
            }
            field.focus.clone()
        });
        window.focus(&focus, cx);
    }
}
