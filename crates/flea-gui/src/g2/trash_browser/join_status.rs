//! The committed feedback message service owns arm expiry and dismissal; the Trash view owns whether the pair is armed.
use crate::{g2::status_feedback::line::ArmLost, state::Shell};
use flea::ui_logic_status_feedback::status::Hint;
use gpui::{Context, Window};

impl Shell {
    pub(super) fn watch_trash_messages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let host = self.trash.as_ref().expect("mounted");
        let messages = host.messages.clone();
        let pane = host.pane.clone();
        let arm = cx.subscribe(&messages, move |_, _, _: &ArmLost, cx| {
            pane.update(cx, |pane, cx| {
                pane.host.view.arm.clear();
                pane.pump(cx);
            });
        });
        let drawn = cx.observe_in(&messages, window, |shell, _, _, cx| {
            shell.sync_trash_message(cx)
        });
        self.trash.as_mut().expect("mounted").message_watches = vec![arm, drawn];
    }

    pub(crate) fn trash_say(&mut self, text: &str, error: bool, cx: &mut Context<Self>) {
        self.trash_say_detail(text, "", error, cx);
    }

    pub(super) fn trash_say_detail(
        &mut self,
        text: &str,
        detail: &str,
        error: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &self.trash else {
            return;
        };
        host.messages.update(cx, |line, cx| {
            line.update(cx, |messages, now| {
                messages.say(text, error, detail, Hint::None, now)
            });
        });
    }

    /// Sidebar.qml onFailed: an open Trash says it on its own bar, else the window's bar says it while the rail shows the count.
    pub(super) fn trash_monitor_failed(&mut self, text: &str, cx: &mut Context<Self>) {
        if self.trash_opened(cx) {
            self.trash_say_detail(text, "", true, cx);
        } else if self
            .rail
            .as_ref()
            .is_some_and(|rail| rail.places.read(cx).model.borrow().trash_count_shown)
        {
            self.status_messages(cx, |messages, now| {
                messages.say(text, true, "", Hint::None, now)
            });
            cx.notify();
        }
    }

    pub(super) fn trash_arm(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(host) = &self.trash else {
            return;
        };
        host.messages.update(cx, |line, cx| {
            line.update(cx, |messages, now| {
                messages.set_arm_owner(1, now.max(1), now);
                messages.say_arm(
                    text,
                    flea::ui_logic_trash_browser::words::ARM_MS as u64,
                    now,
                );
            });
        });
    }

    pub(super) fn trash_dismiss(&mut self, cx: &mut Context<Self>) {
        let Some(host) = &self.trash else {
            return;
        };
        host.messages.update(cx, |line, cx| {
            line.update(cx, |messages, now| messages.dismiss(now));
        });
    }

    /// TrashHost.qml open: any action into a closed Trash opens it and drops the selection of the pane under it.
    pub(super) fn clear_under_trash(&mut self, cx: &gpui::App) {
        let opens = self.trash.as_ref().is_some_and(|host| {
            let view = host.pane.read(cx).view();
            !view.opened && !view.confirming
        });
        if !opens {
            return;
        }
        if let Some(under) = self.panes[self.focus_side].as_mut() {
            under.input.selection.clear(&mut under.selected);
        }
    }

    pub(crate) fn trash_message(&self) -> Option<gpui::SharedString> {
        let host = self.trash.as_ref()?;
        (host.side == self.focus_side && self.slots.covers[host.side].is_some())
            .then(|| host.drawn_message.clone())
    }

    pub(super) fn sync_trash_message(&mut self, cx: &mut Context<Self>) {
        let Some(host) = &self.trash else {
            return;
        };
        let pane = host.pane.read(cx);
        let opened = pane.view().opened;
        if !pane.view().arm.armed() && host.messages.read(cx).messages().arm_live() {
            host.messages.update(cx, |line, cx| {
                line.update(cx, |messages, now| messages.owner_stamp(0, now));
            });
        }
        let line = host.messages.read(cx);
        // StatusBar.qml escapePressed takes only a standing error; a plain note leaves Escape to close Trash.
        host.dismissible
            .set(line.messages().shown().is_error && !line.messages().arm_live());
        if !opened || host.presentation.stage == super::presentation::Stage::Ready {
            let primary = line.drawn().primary.clone();
            self.trash.as_mut().expect("mounted").drawn_message = primary;
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "fixlogic3_tests.rs"]
mod fixlogic3_tests;
