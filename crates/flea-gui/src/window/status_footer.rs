//! Window notes reach the message line the footer currently paints.
use crate::state::Shell;
use flea::ui_logic_status_feedback::status::Hint;
use gpui::Context;

pub(super) fn note(shell: &mut Shell, text: &str, cx: &mut Context<Shell>) {
    shell.sync_footer_owner(cx);
    let footer = shell.footer(cx);
    let line = footer.read_with(cx, |footer, _| footer.message_line());
    line.update(cx, |line, cx| {
        line.update(cx, |messages, now| {
            messages.say(text, false, "", Hint::None, now)
        });
    });
}
