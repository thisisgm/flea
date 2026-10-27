//! Menu and operation producer replies retain their existing routing and observer delivery.
use super::slow_sentence;
use crate::state::Shell;
use gpui::{Context, Window};

impl Shell {
    pub(super) fn menu_producer(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        if self.fileops_line(side, line, window, cx) {
            return;
        }
        let menu_taken = self.menu_reply(side, line, window, cx);
        if let Some(parsed) = flea::ui_logic_op1::parse(line) {
            self.operation_line(side, line, window, cx);
            if let flea::ui_logic_op1::Event::TransferDone(done) = parsed {
                self.retry_failed(side, &done.retry_paths, window, cx);
            }
        }
        if !menu_taken {
            cx.emit(crate::g2::mn1::join::ProducerReply {
                side,
                line: line.to_owned(),
            });
        }
    }
}
