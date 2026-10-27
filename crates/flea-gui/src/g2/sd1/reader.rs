//! The focused pane's provider reader observes its live requests, identity, focus and bounded status jobs.
use crate::{ipc::Adapter, state::Shell};
use flea::{
    ipc::wire::Reply,
    jsondoc::{self, Json},
    ui_logic_dg1::row_path,
};
use gpui::Window;

pub(crate) struct State<'a> {
    pub shell: &'a Shell,
    pub window: &'a Window,
}

fn text(value: &str) -> Json {
    Json::Str(value.into())
}
fn number(value: usize) -> Json {
    Json::Num(value.to_string())
}

impl Adapter for State<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        if function != "providerState" {
            return None;
        }
        let shell = self.shell;
        let side = shell.focus_side;
        let host = shell.panes[side].as_ref()?;
        let providers = &shell.menu.providers;
        let sender = &providers.sides[side];
        let account = &providers.account[side];
        let base = host.search.owner.session.base_of(&host.path);
        let path = |index| {
            host.pane
                .held
                .row(index)
                .map(|row| row_path(base, &row.name))
        };
        let selected = host.selected.iter().map(|&row| number(row)).collect();
        let paths = host
            .selected
            .iter()
            .map(|&row| path(row).map_or(Json::Null, Json::Str))
            .collect();
        let captured = shell
            .menu
            .captured
            .as_ref()
            .is_some_and(|capture| capture.side == side);
        let keyboard = shell.focus.is_focused(self.window);
        let metadata_busy = providers.asked[side] != 0;
        let destination_busy = captured
            && sender.row_on(flea::ui_logic_sd1::providers::Role::Dropbox)
            && !account.path.is_empty()
            && providers.destination[side].is_none();
        let peers = sender
            .td_peers
            .iter()
            .map(|peer| {
                crate::visual::object([
                    ("label", text(&peer.label)),
                    ("id", text(&peer.id)),
                    (
                        "address",
                        text(sender.td_addresses.get(&peer.id).map_or("", String::as_str)),
                    ),
                ])
            })
            .collect();
        let reason = if sender.db.checking {
            shell.locale.catalog.tr("send.providers.dropbox_checking")
        } else if !account.reason.is_empty() {
            account.reason.clone()
        } else {
            sender.db.status.reason.clone()
        };
        let dropbox = crate::visual::object([
            ("checking", Json::Bool(sender.db.checking)),
            (
                "ready",
                Json::Bool(sender.dropbox_ready() && !account.path.is_empty()),
            ),
            ("metadataBusy", Json::Bool(metadata_busy)),
            ("reason", Json::Str(reason)),
            ("path", text(&account.path)),
            (
                "timeoutSeconds",
                number(super::dropbox_status::TIMEOUT_SECONDS as usize),
            ),
        ]);
        let value = crate::visual::object([
            (
                "facts",
                providers.facts[side]
                    .clone()
                    .unwrap_or_else(|| Json::Obj(Vec::new())),
            ),
            (
                "formatsRequests",
                number(
                    host.backend
                        .as_ref()
                        .map_or(0, |backend| backend.formats_requests()),
                ),
            ),
            (
                "refreshing",
                Json::Bool(
                    metadata_busy || sender.td_checking || sender.db.checking || destination_busy,
                ),
            ),
            (
                "pendingActivation",
                Json::Bool(captured && shell.menu.pending.is_some()),
            ),
            (
                "menuFocus",
                Json::Bool(
                    captured
                        && keyboard
                        && shell.menu_opened()
                        && shell.keyboard.context == flea::keys::Context::Menu,
                ),
            ),
            (
                "listFocus",
                Json::Bool(keyboard && shell.keyboard.context == flea::keys::Context::Listing),
            ),
            ("path", text(&host.path)),
            ("cursor", number(host.pane.cursor)),
            (
                "cursorPath",
                Json::Str(path(host.pane.cursor).unwrap_or_default()),
            ),
            ("selected", Json::Arr(selected)),
            ("selectedPaths", Json::Arr(paths)),
            (
                "taildrop",
                crate::visual::object([
                    ("checking", Json::Bool(sender.td_checking)),
                    ("reason", text(&sender.td_reason)),
                    ("peers", Json::Arr(peers)),
                    ("timeoutSeconds", number(super::picker::STATUS_TIMEOUT_SECS)),
                ]),
            ),
            ("dropbox", dropbox),
        ]);
        Some(Reply::value(jsondoc::render(&value).trim_end()))
    }
}
