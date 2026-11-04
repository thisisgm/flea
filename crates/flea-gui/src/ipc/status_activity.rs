//! statusActivityState: the status lane's activities, queued errors, notice and transfer card.
use super::status::StatusAdapter;
use crate::visual::{number, object};
use flea::jsondoc::Json;

impl StatusAdapter<'_> {
    pub(super) fn activity_state(&self, line: &crate::g2::fb1::line::MessageLine) -> Json {
        let messages = line.messages();
        object([
            (
                "activities",
                Json::Arr(
                    messages
                        .activities()
                        .iter()
                        .map(|a| {
                            let side = usize::try_from(a.owner).ok();
                            let host = side
                                .and_then(|s| self.shell.panes.get(s))
                                .and_then(Option::as_ref);
                            object([
                                ("id", number(a.transfer)),
                                ("text", Json::Str(a.text.clone())),
                                ("running", Json::Bool(a.running)),
                                ("cancelling", Json::Bool(a.cancelling)),
                                (
                                    "ownerPath",
                                    Json::Str(host.map(|h| h.path.clone()).unwrap_or_default()),
                                ),
                                (
                                    "ownerFocused",
                                    Json::Bool(
                                        host.is_some() && side == Some(self.shell.focus_side),
                                    ),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("errors", number(messages.errors().len())),
            (
                "notice",
                Json::Str(messages.notice_text(&self.shell.locale.catalog)),
            ),
            (
                "undoAvailable",
                Json::Bool(
                    self.shell
                        .transfer_undo_available(messages.notice(), self.cx)
                        .unwrap_or(!line.drawn().undo.is_empty()),
                ),
            ),
            (
                "transferCard",
                super::status_card::read(self.shell, self.cx, line),
            ),
        ])
    }
}
