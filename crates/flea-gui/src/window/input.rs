use super::{lifecycle::Phase, startup};
use crate::{
    list::pane::{Move, ROW_HEIGHT},
    state::Shell,
};
use gpui::{px, Context, KeyDownEvent, ScrollWheelEvent, Window};
#[path = "keyboard.rs"]
mod keyboard;
pub use keyboard::*;

#[derive(Clone, Copy, Debug)]
pub struct ActionEvent {
    pub action: &'static str,
    pub context: flea::keys::Context,
    pub seat: flea::keys::sequence::Seat,
    pub pair_armed: bool,
}

impl gpui::EventEmitter<ActionEvent> for Shell {}

impl Shell {
    pub fn new_window(&self) {
        let Some(host) = &self.panes[self.focus_side] else {
            return;
        };
        let mut command = startup::new_window_command(&host.path);
        let locale = self.locale.clone();
        let _ = std::thread::Builder::new()
            .name("flea-window".into())
            .spawn(move || {
                if let Err(error) = command.spawn().and_then(|mut child| child.wait()) {
                    eprintln!(
                        "flea-gui: {}",
                        locale
                            .catalog
                            .tr_args("window.open_failed", &[&error.to_string()])
                    );
                }
            });
    }

    pub(crate) fn key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.phase() != Phase::Open {
            return;
        }
        if modifiers(event.keystroke.modifiers) == 0 {
            if let Some(host) = &mut self.panes[self.focus_side] {
                // A pane mid-swap swallows plain keys, so no key acts on the listing being replaced.
                if host.pane.busy() {
                    cx.stop_propagation();
                    return;
                }
                if self.seam.key(&event.keystroke.key, &mut host.pane) {
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
            }
        }
        let Some(host) = &self.panes[self.focus_side] else {
            return;
        };
        let seat = flea::keys::sequence::Seat {
            listing: host.pane.held.listing as u64,
            cursor: host.pane.cursor,
            view: self.view as u64,
            focus: self.focus_side as u64,
            ..Default::default()
        };
        let name = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default");
        let pair_armed = self.keyboard.pair_armed(name, seat);
        let action = match self.keyboard.resolve(name, event, window.modifiers(), seat) {
            Dispatch::Unbound | Dispatch::Composition => return,
            Dispatch::Suppressed | Dispatch::Armed => {
                cx.stop_propagation();
                return;
            }
            Dispatch::Action(action) => action,
        };
        if action == "windowNew" {
            self.new_window();
        } else if let Some(view) = match action {
            "viewList" => Some(startup::View::List),
            "viewColumns" => Some(startup::View::Columns),
            "viewGrid" => Some(startup::View::Grid),
            "viewDual" => Some(startup::View::Dual),
            _ => None,
        } {
            self.set_view(view, window, cx);
        } else {
            let step = match action {
                "cursorUp" => Some(Move::Up),
                "cursorDown" => Some(Move::Down),
                "pageUp" => Some(Move::PageUp),
                "pageDown" => Some(Move::PageDown),
                "cursorFirst" => Some(Move::Home),
                "cursorLast" => Some(Move::End),
                _ => None,
            };
            if let Some(step) =
                step.filter(|_| self.keyboard.context == flea::keys::Context::Listing)
            {
                let host = self.panes[self.focus_side].as_mut().expect("focused pane");
                if !host.pane.busy() && host.pane.move_cursor(step) {
                    self.seam.selected(host.pane.cursor);
                    cx.notify();
                }
            } else {
                cx.emit(ActionEvent {
                    action,
                    seat,
                    context: self.keyboard.context,
                    pair_armed,
                });
            }
        }
        cx.stop_propagation();
    }

    pub(crate) fn wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.lifecycle.phase() != Phase::Open {
            return;
        }
        if let Some(host) = &mut self.panes[self.focus_side] {
            if host.pane.busy() {
                return;
            }
            host.pane
                .scroll_by(-f32::from(event.delta.pixel_delta(px(ROW_HEIGHT)).y));
            cx.notify();
        }
    }
}
