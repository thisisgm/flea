//! The window's provider sends: one ProviderSend per pane backend, its rows reaching mn1 through the menu Context.
use super::{
    seams::{Action, Wire},
    ProviderSend,
};
use crate::{backend::Backend, backend::Event as BackendEvent, state::Shell};
use flea::{
    json::escape,
    jsondoc::{self, Json},
    ui_logic_context_menus::build::Context as MenuContext,
    ui_logic_file_ops::collide::{self, Request},
    ui_logic_send_share::{
        dropbox,
        providers::{self, Role},
    },
};
use gpui::{Context, Subscription, Window};
#[path = "join_process.rs"]
mod process;

/// Each pane backend numbers its own requests, so each keeps its own ProviderSend and facts ask.
#[derive(Default)]
pub(crate) struct Senders {
    pub(crate) sides: [ProviderSend; 2],
    // The menu snapshot each side's providers facts ask belongs to; zero while none is out.
    pub(super) asked: [u64; 2],
    pub(super) facts: [Option<Json>; 2],
    // The Dropbox account the facts named, and whether the backend took it as the menu's destination.
    pub(super) account: [dropbox::Account; 2],
    pub(super) destination: [Option<bool>; 2],
    pub(super) activation: Option<super::activation::Activation>,
    // Dropped first: a closing window waits for owly's teardown before it lets go of the machine's radio.
    pub(crate) airdrop_pids: super::airdrop_pids::Pids,
    // Omdrop's lookups; its on and off are never held here.
    pub(crate) omdrop_pids: super::airdrop_pids::Pids,
    pub(crate) airdrop: super::airdrop::AirDrop,
    // AirDrop when Omdrop runs it in place of owly.
    pub(crate) omdrop: super::airdrop_omdrop::Omdrop,
    // The host name the AirDrop page shows, read each time Settings shows the page.
    pub(crate) airdrop_host: String,
    chosen: Option<Subscription>,
}

// The pane backend's ordered pipe; false when that backend is gone.
pub(super) struct Pipe<'a>(pub(super) Option<&'a Backend>);

impl Wire for Pipe<'_> {
    fn send(&self, line: String) -> bool {
        self.0.is_some_and(|backend| backend.line(line).is_ok())
    }
}

pub(super) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

impl Shell {
    // A file menu opened: ask this side's backend for the providers facts under the menu's own id.
    pub(crate) fn providers_menu_opened(
        &mut self,
        side: usize,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.observe_providers(window, cx);
        self.rclone_menu_opened(side);
        self.provider_query(side, id, cx);
    }

    pub(crate) fn providers_menu_closed(&mut self) {
        self.menu.providers.activation = None;
        self.airdrop_menu_closed();
        for sender in &self.menu.rclone {
            sender.borrow_mut().menu_closed();
        }
        self.retire_plugin_menu();
    }

    // Sample input: {"t":"menuaction","id":7,"op":"providerDestination","ok":true} (dead backend: sends lost).
    pub(crate) fn provider_event(
        &mut self,
        side: usize,
        event: &BackendEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let line = match event {
            BackendEvent::Producer(line) | BackendEvent::Operations(line) => line,
            BackendEvent::Failed(_) | BackendEvent::Drained => {
                self.menu.rclone[side].borrow_mut().backend_ended();
                self.menu.providers.sides[side].backend_dead();
                self.menu.providers.asked[side] = 0;
                self.menu.providers.destination[side] = None;
                if self
                    .menu
                    .providers
                    .activation
                    .as_ref()
                    .is_some_and(|a| a.side == side)
                {
                    self.menu.providers.activation = None;
                }
                return false;
            }
            _ => return false,
        };
        let Ok(reply) = jsondoc::parse(line) else {
            return false;
        };
        let id = match reply.get("id") {
            Some(Json::Num(value)) => value.parse::<u64>().unwrap_or(0),
            _ => 0,
        };
        let kind = reply.get("t").and_then(Json::as_str).unwrap_or("");
        if kind == "airdropsent" {
            self.airdrop_sent(side, line, window, cx);
        }
        if self.plugin_selection_reply(side, &reply) {
            self.provider_refresh(window, cx);
        }
        let asked = self.menu.providers.asked[side];
        if kind == "formats" && asked != 0 && id == asked {
            self.menu.providers.asked[side] = 0;
            // The same reply names the archive formats and extract tools Compress and Extract read.
            self.record_formats(side, &reply);
            self.menu.providers.facts[side] = reply.get("providers").cloned();
            // A menu dismissed before its facts landed starts no discovery.
            if self
                .menu
                .captured
                .as_ref()
                .is_some_and(|c| c.side == side && c.id == id)
            {
                self.provider_facts(side, &reply, window, cx);
            }
            return true;
        }
        if kind == "menuaction"
            && reply.get("op").and_then(Json::as_str) == Some("providerDestination")
        {
            let current = self
                .menu
                .captured
                .as_ref()
                .is_some_and(|c| c.side == side && c.id == id);
            if current {
                self.menu.providers.destination[side] =
                    Some(reply.get("ok").and_then(Json::as_bool) == Some(true));
                self.provider_refresh(window, cx);
            }
            return true;
        }
        let actions = self.menu.providers.sides[side].line(line, &self.locale.catalog, now_ms());
        let peers = matches!(kind, "localsendpeers");
        self.provider_actions(side, actions, window, cx);
        if peers {
            self.provider_refresh(window, cx);
        }
        false
    }

    fn provider_facts(
        &mut self,
        side: usize,
        reply: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let raw = reply
            .get("providers")
            .map(jsondoc::render)
            .unwrap_or_default();
        let Ok(mut facts) = providers::parse_facts(&raw) else {
            return;
        };
        let info = reply
            .get("providers")
            .and_then(|p| p.get("dropboxInfo"))
            .and_then(Json::as_str);
        let error = reply
            .get("providers")
            .and_then(|p| p.get("dropboxError"))
            .and_then(Json::as_str);
        self.menu.providers.account[side] =
            dropbox::account(info.unwrap_or(""), error.unwrap_or(""));
        facts.dropbox_ready &= !self.menu.providers.account[side].path.is_empty();
        let states = self.declared_provider_states();
        let hidden = self.menu_hidden();
        let only = self.menu.providers.activation_role(side);
        let backend = self.panes[side]
            .as_ref()
            .and_then(|host| host.backend.as_ref());
        let actions = self.menu.providers.sides[side].menu_refresh(
            &Pipe(backend),
            &facts,
            Some(&states),
            &hidden,
            now_ms(),
            only,
        );
        self.provider_actions(side, actions, window, cx);
        let tools = (facts.owly_command.as_str(), facts.omdrop_command.as_str());
        self.airdrop_facts(side, tools, window, cx);
        let path = self.menu.providers.account[side].path.clone();
        let capture = self
            .menu
            .captured
            .clone()
            .filter(|c| c.side == side && !c.background);
        if let (Some(capture), false) = (
            capture,
            path.is_empty()
                || !self.menu.providers.sides[side].row_on(Role::Dropbox)
                || !self.menu.providers.destination_needed(side),
        ) {
            self.menu_send(
                side,
                format!(
                    "{{\"c\":\"menuaction\",\"op\":\"providerDestination\",\"id\":{},\"dest\":\"{}\"}}",
                    capture.id,
                    escape(&path)
                ),
                cx,
            );
        }
        self.provider_refresh(window, cx);
    }

    // The provider rows mn1 draws, from this side's sender: rows show only when usable and on.
    pub(crate) fn provider_context(&self, side: usize, folder: &str, context: &mut MenuContext) {
        let sender = &self.menu.providers.sides[side];
        let declared = |role: Role| {
            self.declared_menu_plugins("send-target")
                .any(|p| p.manifest.id == role.plugin_id())
        };
        context.taildrop_installed = sender.row_on(Role::Tailscale) && declared(Role::Tailscale);
        context.taildrop_peers = sender.peers_for(Role::Tailscale);
        context.taildrop_refreshing = sender.taildrop_refreshing();
        context.taildrop_code = sender.taildrop_code().into();
        context.localsend_installed = sender.row_on(Role::LocalSend) && declared(Role::LocalSend);
        context.localsend_peers = sender.peers_for(Role::LocalSend);
        context.localsend_checking = sender.localsend_checking() && !sender.localsend_answered();
        let account = &self.menu.providers.account[side];
        context.dropbox_installed = sender.row_on(Role::Dropbox) && declared(Role::Dropbox);
        context.dropbox_refreshing = sender.db.checking
            || (!account.path.is_empty() && self.menu.providers.destination[side].is_none());
        context.dropbox_path = match self.menu.providers.destination[side] {
            Some(true) if sender.dropbox_ready() => account.path.clone(),
            _ => String::new(),
        };
        context.dropbox_code = match self.menu.providers.destination[side] {
            Some(false) => String::new(),
            _ if !account.code.is_empty() => account.code.clone(),
            _ => sender.db.status.code.clone(),
        };
        let row = format!("{}/{}", folder.trim_end_matches('/'), context.row_name);
        context.row_in_dropbox = dropbox::contains(&account.path, &row);
        self.airdrop_context(side, context);
    }

    // The Dropbox root a dropbox or sharelink validate names, so the backend checks the same folder.
    pub(crate) fn provider_destination(&self, side: usize) -> String {
        self.menu.providers.account[side].path.clone()
    }

    // A provider answer under an open menu rebuilds its rows in place; before the card shows, the load path does.
    pub(crate) fn provider_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.menu_opened() {
            self.refresh_menu(window, cx);
            self.provider_activation_ready(window, cx);
            return;
        }
        let Some(side) = self.menu.captured.as_ref().map(|c| c.side) else {
            return;
        };
        let entries = self.menu_entries();
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let list = host.list.borrow();
        let env = self.menu_env(&list.typeface);
        let changed = self.menu.menu.borrow_mut().refresh(entries, &env, window);
        drop(list);
        if changed {
            cx.notify();
        }
        self.provider_activation_ready(window, cx);
    }

    fn observe_providers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.providers.chosen.is_some() {
            return;
        }
        let entity = cx.entity();
        self.menu.providers.chosen = Some(cx.subscribe_in(
            &entity,
            window,
            |shell, _, chosen: &crate::g2::context_menus::join::Chosen, window, cx| {
                shell.provider_chosen(chosen, window, cx);
            },
        ));
    }

    // A checked provider row: claim it, dispatch through this side's sender, then close the snapshot.
    pub(crate) fn provider_chosen(
        &mut self,
        chosen: &crate::g2::context_menus::join::Chosen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = chosen.captured.side;
        if !matches!(
            chosen.action.as_str(),
            "localsend" | "taildrop" | "sharelink" | "dropbox" | "airdrop"
        ) || chosen.captured.background
            || self.panes.get(side).and_then(Option::as_ref).is_none()
            || !chosen.claim()
        {
            return;
        }
        let catalog = self.locale.catalog.clone();
        let id = chosen.captured.id;
        let actions = match chosen.action.as_str() {
            "localsend" => {
                let backend = self.panes[side]
                    .as_ref()
                    .and_then(|host| host.backend.as_ref());
                self.menu.providers.sides[side].send_localsend(
                    &Pipe(backend),
                    &chosen.leaf,
                    &chosen.paths,
                    &catalog,
                )
            }
            "airdrop" => {
                self.airdrop_chosen(side, &chosen.leaf, &chosen.paths, window, cx);
                Vec::new()
            }
            "taildrop" => chosen
                .paths
                .iter()
                .flat_map(|path| {
                    self.menu.providers.sides[side].send_taildrop(
                        &chosen.leaf,
                        std::slice::from_ref(path),
                        &catalog,
                    )
                })
                .collect(),
            "sharelink" => {
                let path = if chosen.paths.len() == 1 {
                    chosen.paths[0].as_str()
                } else {
                    ""
                };
                self.menu.providers.sides[side].copy_share_link(path, &catalog)
            }
            _ => {
                let dest = self.menu.providers.account[side].path.clone();
                if self.menu.providers.destination[side] == Some(true)
                    && !dest.is_empty()
                    && self.menu.providers.sides[side].dropbox_ready()
                {
                    let request = Request::new()
                        .with("c", &collide::text("transfer"))
                        .with("op", &collide::text("move"))
                        .with("menuId", &id.to_string())
                        .with("dest", &collide::text(&dest));
                    self.ask_operation(side, &request, &[], false, window, cx);
                    Vec::new()
                } else {
                    vec![Action::Say(catalog.tr("send.providers.menu_stale"), true)]
                }
            }
        };
        self.menu_send(
            side,
            format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{id}}}"),
            cx,
        );
        self.provider_actions(side, actions, window, cx);
    }
}

#[cfg(test)]
impl Shell {
    pub(crate) fn plugin_fixture_dropbox(&mut self, generation: u64, stdout: &str) {
        self.menu.providers.sides[0].dropbox_done(generation, 0, stdout, "", &self.locale.catalog);
        self.menu.providers.account[0].path = "/Dropbox".into();
        self.menu.providers.destination[0] = Some(true);
    }
}
