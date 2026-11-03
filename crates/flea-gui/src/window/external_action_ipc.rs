//! `flea --action` and `flea --actions` arrive on the window's IPC reader and run on the shared action path.
use crate::{ipc::server::Answer, state::Shell, window::external_action::Waiting};
use flea::ipc::wire::{Reply, Request};
use gpui::{Context, Window};
use std::sync::mpsc::SyncSender;

/// True for a well-formed action call; anything else keeps the ordinary reader's answer.
pub(crate) fn is_action_call(request: &Request) -> bool {
    matches!(request, Request::Call { target, function, arguments }
        if target == "flea" && matches!((function.as_str(), arguments.len()), ("runAction", 1) | ("actionIds", 0)))
}

impl Shell {
    pub(crate) fn action_call(
        &mut self,
        request: Request,
        answer: SyncSender<Answer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let waiting = Waiting::new(answer, self.action_fallback());
        let Request::Call {
            function,
            arguments,
            ..
        } = request
        else {
            return;
        };
        if function == "actionIds" {
            let ids = self.action_ids(window, cx);
            waiting.answer(Reply::value(ids.join("\n")));
            return;
        }
        let id = arguments.into_iter().next().unwrap_or_default();
        self.run_action(&id, waiting, window, cx);
    }
}
