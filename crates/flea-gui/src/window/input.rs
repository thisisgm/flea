use super::{lifecycle::Phase, startup};
use crate::{
    list::pane::{Move, ROW_HEIGHT},
    state::Shell,
};
use gpui::{px, Context, KeyDownEvent, ScrollWheelEvent, Window};

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

    pub(crate) fn key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.lifecycle.phase() != Phase::Open {
            return;
        }
        if event.keystroke.modifiers.control && event.keystroke.key == "n" {
            self.new_window();
            cx.stop_propagation();
            return;
        }
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.platform
        {
            return;
        }
        let Some(host) = &mut self.panes[self.focus_side] else {
            return;
        };
        if host.pane.busy() {
            cx.stop_propagation();
            return;
        }
        let step = match event.keystroke.key.as_str() {
            "up" => Move::Up,
            "down" => Move::Down,
            "pageup" => Move::PageUp,
            "pagedown" => Move::PageDown,
            "home" => Move::Home,
            "end" => Move::End,
            key => {
                if self.seam.key(key, &mut host.pane) {
                    cx.stop_propagation();
                    cx.notify();
                }
                return;
            }
        };
        if host.pane.move_cursor(step) {
            self.seam.selected(host.pane.cursor);
            cx.stop_propagation();
            cx.notify();
        }
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
