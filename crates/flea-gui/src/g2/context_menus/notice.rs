//! Menu notices: a failed backend send, a refused choice and a menu that explains why it did not open.
use super::join::Captured;
use crate::{backend::Failure, state::Shell};
use gpui::Context;

impl Shell {
    pub(crate) fn menu_send(&mut self, side: usize, line: String, cx: &mut Context<Self>) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let result = host
            .backend
            .as_ref()
            .ok_or_else(|| Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            })
            .and_then(|backend| backend.line(line));
        if let Err(error) = result {
            self.menu.tags.pending[side].clear();
            self.menu.tags.retire();
            host.record_error(error, &self.locale);
            self.menu.pending = None;
            self.menu.activating = false;
            self.menu.reading = false;
            self.menu.menu.borrow_mut().close();
            self.menu.captured = None;
            self.menu.presentation.cancel();
            self.keyboard.context = flea::keys::Context::Listing;
            host.nav.gate.menu_open = false;
            host.nav.gate.menu_pending = false;
            let sentence = host.status.to_string();
            self.menu_notice(side, sentence, cx);
            cx.notify();
        }
    }

    // Pane.qml performLocked's last line: a Locked tile row nobody ran says so plainly; any other menu asks to reopen it.
    pub(crate) fn menu_unclaimed(&mut self, capture: &Captured, cx: &mut Context<Self>) {
        let side = capture.side;
        if capture.background && self.locked_folder(side).is_some() {
            let text = self.locale.catalog.tr("feedback.action.unavailable");
            return self.menu_explain(text, cx);
        }
        if !capture.background {
            let close = format!(
                "{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{}}}",
                capture.id
            );
            self.menu_send(side, close, cx);
        }
        self.menu_notice(side, self.locale.catalog.tr("menu.refusal.unavailable"), cx);
    }

    pub(crate) fn menu_refuse(&mut self, key: &str, cx: &mut Context<Self>) {
        let side = self
            .menu
            .captured
            .as_ref()
            .map_or(self.focus_side, |c| c.side);
        self.menu_notice(side, self.locale.catalog.tr(key), cx);
    }

    pub(crate) fn menu_notice(&mut self, side: usize, sentence: String, cx: &mut Context<Self>) {
        if let Some(host) = &mut self.panes[side] {
            host.status = sentence.clone().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.say(
                &sentence,
                true,
                "",
                flea::ui_logic_status_feedback::status::Hint::None,
                now,
            );
        });
        cx.notify();
    }

    /// Why no menu opened, when nothing failed: a foreground notice with the dismiss key after it.
    pub(crate) fn menu_explain(&mut self, sentence: String, cx: &mut Context<Self>) {
        self.status_messages(cx, |messages, now| {
            messages.explain(
                &sentence,
                flea::ui_logic_status_feedback::status::Hint::Dismiss,
                now,
            );
        });
        cx.notify();
    }
}
