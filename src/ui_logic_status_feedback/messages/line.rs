//! The centre as drawn: the shown sentence, its hint split from the rest of the lane, and the undo target.
use super::Messages;
use crate::i18n::I18n;
use crate::ui_logic_status_feedback::status::{
    centre_role, centre_text, displayed_starting, secondary_text, Hint, Role, DISMISS_KEY, SEP,
};

/// The status centre as drawn: primary in its role, the muted lane, the undo target and the error detail.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub primary: String,
    pub error: bool,
    /// The whole muted lane, as one string.
    pub secondary: String,
    /// Its leading key hint, which never elides, and the rest of the lane after it, which may.
    pub hint: String,
    pub extra: String,
    pub undo: String,
    pub detail: String,
    pub busy: bool,
}

impl Messages {
    /// The centre's words; `state_message` is the pane's own state sentence, under which the dismiss key is no news.
    pub fn line(&self, catalog: &I18n, state_message: &str) -> Line {
        let shown = self.shown();
        let slot = self.slot(&shown);
        let hint = if shown.text.is_empty() || slot.sticky_here || slot.searching || self.arm_live()
        {
            Hint::None
        } else if shown.is_error {
            self.errors[0].hint
        } else {
            self.notice_hint
        };
        let text = hint.text(catalog, &self.preset);
        let split = hint.has_undo(&self.preset) && !text.undo.is_empty();
        let hint_rest = if split {
            text.rest.clone()
        } else {
            text.joined()
        };
        let key_hint = if shown.is_error {
            let dismiss = if self.errors[0].text == state_message {
                String::new()
            } else {
                catalog.tr_args("feedback.hint.dismiss", &[&catalog.keycap(DISMISS_KEY)])
            };
            [dismiss, hint_rest]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(SEP)
        } else {
            hint_rest
        };
        let lane = |key_hint: &str| {
            secondary_text(
                key_hint,
                shown.is_error,
                slot.sticky_here,
                slot.sticky,
                &self.activities,
                slot.searching,
                &self.search_line,
                &self.retry_line,
            )
        };
        Line {
            primary: centre_text(&slot).into(),
            error: centre_role(&slot) == Role::Error,
            secondary: lane(&key_hint),
            hint: if key_hint.is_empty() {
                String::new()
            } else {
                format!("{SEP}{key_hint}")
            },
            extra: lane(""),
            undo: if split { text.undo } else { String::new() },
            detail: shown.detail,
            busy: displayed_starting(&self.activities),
        }
    }
}
