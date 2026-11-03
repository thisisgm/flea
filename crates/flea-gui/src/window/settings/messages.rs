use crate::state::Shell;
use flea::ui_logic_fb1::{messages::Escape, status::Hint};
use gpui::{Context, KeyDownEvent, SharedString};

impl Shell {
    pub(super) fn settings_error(&mut self, key: &'static str, cx: &mut Context<Self>) {
        let footer = self.footer(cx);
        self.settings.messages = Some(footer.read_with(cx, |footer, _| footer.line.clone()));
        let text = self.locale.catalog.tr(key);
        self.settings
            .messages
            .as_ref()
            .unwrap()
            .update(cx, |line, cx| {
                line.update(cx, |messages, now| {
                    messages.say(&text, true, "", Hint::None, now)
                });
            });
        cx.notify();
    }

    pub(crate) fn settings_message(&self, cx: &gpui::App) -> Option<SharedString> {
        self.settings.messages.as_ref()?.read_with(cx, |line, _| {
            (!line.drawn().primary.is_empty()).then(|| line.drawn().primary.clone())
        })
    }

    pub(crate) fn settings_dismiss(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.keyboard.context != flea::keys::Context::Listing
            || self.settings.opened
            || self.settings.sheet.is_some()
            || event.keystroke.key != "escape"
            || crate::window::input::modifiers(event.keystroke.modifiers) != 0
        {
            return false;
        }
        let Some(line) = &self.settings.messages else {
            return false;
        };
        let dismissed = line.update(cx, |line, cx| {
            line.update(cx, |messages, now| messages.escape(false, now))
        }) != Escape::Ignored;
        if dismissed {
            cx.notify();
            cx.stop_propagation();
        }
        dismissed
    }
}
