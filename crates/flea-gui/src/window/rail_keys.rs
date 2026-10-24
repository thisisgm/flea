//! The rail's keys while it holds focus: the sidebar toggle, the eject mark and Ctrl+E, then the row cursor.
use super::*;
use gpui::KeyDownEvent;

impl Shell {
    pub(crate) fn rail_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        use crate::window::input::{event_input, Dispatch};
        use flea::keys::{self, sequence::Seat, Frontend, Match};
        let Some(join) = &self.rail else {
            return None;
        };
        let focused =
            self.dual_focus.rail.contains_focused(window, cx) || join.focus_pending == Some(true);
        let producer = join.places.clone();
        if focused && producer.read(cx).rename_index().is_some() {
            producer.update(cx, |producer, _| producer.side = self.focus_side);
            return Some(producer.update(cx, |producer, cx| producer.rename_key(event, cx)));
        }
        let name = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(Json::as_str)
            .unwrap_or("default");
        let action = keys::lookup(
            name,
            self.keyboard.context,
            Frontend::Gui,
            event_input(event, window.modifiers()),
        );
        if action == Match::Action("sidebar") {
            if join.host.read(cx).rail.mode() == Mode::Overlay {
                return Some(true);
            }
            let hide = join.host.read(cx).rail.mode() != Mode::Hidden;
            let patch = Json::Obj(vec![(
                "places".into(),
                Json::Obj(vec![(
                    "rail".into(),
                    Json::Str(if hide { "hidden" } else { "shown" }.into()),
                )]),
            )]);
            self.change_settings(patch, window, cx);
            return Some(true);
        }
        if matches!(action, Match::Action(name) if matches!(name,
            "focusNext" | "focusPrevious" | "focusPreview" | "keymapSheet" |
            "pathBar" | "windowNew" | "reload" | "openTerminal" | "settings" |
            "copydirpath") || name.starts_with("tab") || name.starts_with("textSize"))
        {
            // U03/U26 retain the registered stop order and consecutive dual pane stops.
            return None;
        }
        if !focused {
            return None;
        }
        if action == Match::Action("addNetwork") {
            let event = crate::window::input::ActionEvent::bare("addNetwork", flea::keys::Context::Rail);
            self.dispatch(Order::Action(event), window, cx);
            return Some(true);
        }
        producer.update(cx, |producer, _| producer.side = self.focus_side);
        let stroke = &event.keystroke;
        let plain = !(stroke.modifiers.control || stroke.modifiers.alt || stroke.modifiers.platform);
        if producer.update(cx, |producer, cx| producer.mark_key(&stroke.key, plain, cx)) {
            return Some(true);
        }
        let seat = Seat {
            focus: 2,
            cursor: producer.read(cx).cursor().unwrap_or(0),
            ..Seat::default()
        };
        if let Dispatch::Action(action) =
            self.keyboard.resolve(name, event, window.modifiers(), seat)
        {
            if action == "eject" {
                // RailKeys.act: Ctrl+E from the rail releases the cursor row.
                self.release_device(false, cx);
            } else if action == "escape" {
                self.rail_focus(false, cx);
                window.focus(&self.focus, cx);
                self.keyboard.context = flea::keys::Context::Listing;
            } else {
                producer.update(cx, |producer, cx| producer.key_action(action, cx));
            }
        }
        Some(true)
    }
}
