//! The flea.networkFeedMountinfo seam: a test feeds mountinfo text into the NETWORK rail's rebuild.
use crate::ipc::server::Answer;
use crate::state::Shell;
use flea::ipc::{
    registry,
    wire::{Reply, Request},
};
use gpui::Context;
use std::sync::mpsc::SyncSender;

impl Shell {
    /// Answers the main window's feed call. The reply waits for the rebuild, so a test reads the rail it fed.
    pub(crate) fn network_mountinfo_call(
        &mut self,
        request: &Request,
        answer: &SyncSender<Answer>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Request::Call {
            target,
            function,
            arguments,
        } = request
        else {
            return false;
        };
        if target != "flea" || function != "networkFeedMountinfo" {
            return false;
        }
        let reply = match registry::validate(target, function, arguments) {
            Err(reply) => reply,
            Ok(_) if self.network().is_none() => Reply::value("no service"),
            Ok(_) => {
                self.network_rebuild(Some(arguments[0].clone()), Some(answer.clone()), cx);
                return true;
            }
        };
        let _ = answer.try_send(Answer { reply, quit: false });
        true
    }
}
