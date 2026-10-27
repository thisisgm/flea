//! A backend slow-operation notice becomes the status line's sentence.

use crate::state::Shell;
use gpui::Context;

impl Shell {
    pub(super) fn slow_line(&mut self, line: &str, cx: &mut Context<Self>) {
        if let Some(sentence) = slow_sentence(line) {
            self.status_messages(cx, |messages, now| {
                messages.say(
                    &sentence,
                    false,
                    "",
                    flea::ui_logic_fb1::status::Hint::None,
                    now,
                )
            });
        }
    }
}

// Sample input: {"t":"slow","op":"rename","msg":"The rename will finish on its own."}
pub(crate) fn slow_sentence(line: &str) -> Option<String> {
    if !line.contains("\"slow\"") {
        return None;
    }
    let document = flea::jsondoc::parse(line).ok()?;
    (document.get("t")?.as_str()? == "slow")
        .then(|| document.get("msg")?.as_str().map(str::to_owned))
        .flatten()
        .filter(|sentence| !sentence.is_empty())
}
