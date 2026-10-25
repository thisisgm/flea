//! The window's provider sends: one ProviderSend per pane backend, fed by file menu opens, that
//! backend's replies and the host runs its Actions name. Rows reach mn1 through the menu Context.
use super::{
    seams::{Action, ProviderState, RunKind, Wire},
    ProviderSend,
};
use crate::{backend::Backend, backend::Event as BackendEvent, state::Shell};
use flea::{
    json::escape,
    jsondoc::{self, Json},
    ui_logic_mn1::build::Context as MenuContext,
    ui_logic_op1::collide::{self, Request},
    ui_logic_sd1::{
        dropbox,
        providers::{self, Role, PLUGIN_DROPBOX, PLUGIN_LOCALSEND, PLUGIN_TAILSCALE},
    },
    ui_logic_st2::{builtin, store},
};
use gpui::{Context, Subscription, Window};
use std::process::{Command, Stdio};

/// Each pane backend numbers its own requests, so each keeps its own ProviderSend and facts ask.
#[derive(Default)]
pub(crate) struct Senders {
    pub(crate) sides: [ProviderSend; 2],
    // The menu snapshot each side's providers facts ask belongs to; zero while none is out.
    asked: [u64; 2],
    // The Dropbox account the facts named, and whether the backend took it as the menu's destination.
    account: [dropbox::Account; 2],
    destination: [Option<bool>; 2],
    chosen: Option<Subscription>,
}

// The pane backend's ordered pipe; false when that backend is gone.
struct Pipe<'a>(Option<&'a Backend>);

impl Wire for Pipe<'_> {
    fn send(&self, line: String) -> bool {
        self.0.is_some_and(|backend| backend.line(line).is_ok())
    }
}

// The st2 switch for each provider plugin, read from ui.json with its shipped default.
pub(crate) fn provider_states(settings: &Json) -> Vec<ProviderState> {
    let shipped = builtin::manifests();
    [PLUGIN_TAILSCALE, PLUGIN_LOCALSEND, PLUGIN_DROPBOX]
        .iter()
        .map(|id| {
            let on = shipped.iter().any(|(manifest, on)| manifest.id == *id && *on);
            ProviderState {
                plugin: (*id).into(),
                enabled: store::enabled(settings, id, on),
                failed: false,
            }
        })
        .collect()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

impl Shell {
    // A file menu opened: ask this side's backend for the providers facts under the menu's own id.
    pub(crate) fn providers_menu_opened(&mut self, side: usize, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.observe_providers(window, cx);
        self.menu.providers.asked[side] = id;
        self.menu.providers.destination[side] = None;
        self.menu_send(side, format!("{{\"c\":\"formats\",\"id\":{id}}}"), cx);
    }

    pub(crate) fn providers_menu_closed(&mut self) {
        for sender in &mut self.menu.providers.sides {
            sender.menu_closed();
        }
    }

    // One backend event: the facts and destination replies for this menu are taken here, LocalSend
    // replies feed their own side and still reach Jobs, and a dead backend loses its live sends.
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
                self.menu.providers.sides[side].backend_dead();
                self.menu.providers.asked[side] = 0;
                self.menu.providers.destination[side] = None;
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
        let asked = self.menu.providers.asked[side];
        if kind == "formats" && asked != 0 && id == asked {
            self.menu.providers.asked[side] = 0;
            // The same reply names the archive formats and extract tools Compress and Extract read.
            self.record_formats(side, &reply);
            // A menu dismissed before its facts landed starts no discovery.
            if self.menu.captured.as_ref().is_some_and(|c| c.side == side && c.id == id) {
                self.provider_facts(side, &reply, window, cx);
            }
            return true;
        }
        if kind == "menuaction"
            && reply.get("op").and_then(Json::as_str) == Some("providerDestination")
        {
            let current = self.menu.captured.as_ref().is_some_and(|c| c.side == side && c.id == id);
            if current {
                self.menu.providers.destination[side] = Some(reply.get("ok").and_then(Json::as_bool) == Some(true));
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

    fn provider_facts(&mut self, side: usize, reply: &Json, window: &mut Window, cx: &mut Context<Self>) {
        let raw = reply.get("providers").map(jsondoc::render).unwrap_or_default();
        let Ok(facts) = providers::parse_facts(&raw) else {
            return;
        };
        let info = reply.get("providers").and_then(|p| p.get("dropboxInfo")).and_then(Json::as_str);
        let error = reply.get("providers").and_then(|p| p.get("dropboxError")).and_then(Json::as_str);
        self.menu.providers.account[side] = dropbox::account(info.unwrap_or(""), error.unwrap_or(""));
        let states = provider_states(&self.launch.settings.data);
        let hidden = self.menu_hidden();
        let backend = self.panes[side].as_ref().and_then(|host| host.backend.as_ref());
        let actions = self.menu.providers.sides[side].menu_opened(&Pipe(backend), &facts, Some(&states), &hidden, now_ms());
        self.provider_actions(side, actions, window, cx);
        let path = self.menu.providers.account[side].path.clone();
        let capture = self.menu.captured.clone().filter(|c| c.side == side && !c.background);
        if let (Some(capture), false) = (capture, path.is_empty() || !self.menu.providers.sides[side].row_on(Role::Dropbox)) {
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
        context.taildrop_installed = sender.row_on(Role::Tailscale);
        context.taildrop_peers = sender.peers_for(Role::Tailscale);
        context.taildrop_refreshing = sender.taildrop_refreshing();
        context.taildrop_code = sender.taildrop_code().into();
        context.localsend_installed = sender.row_on(Role::LocalSend);
        context.localsend_peers = sender.peers_for(Role::LocalSend);
        context.localsend_checking = sender.localsend_checking() && !sender.localsend_answered();
        let account = &self.menu.providers.account[side];
        context.dropbox_installed = sender.row_on(Role::Dropbox);
        context.dropbox_refreshing = !account.path.is_empty() && self.menu.providers.destination[side].is_none();
        context.dropbox_path = match self.menu.providers.destination[side] {
            Some(true) => account.path.clone(),
            _ => String::new(),
        };
        context.dropbox_code = match self.menu.providers.destination[side] {
            Some(false) => String::new(),
            _ => account.code.clone(),
        };
        let row = format!("{}/{}", folder.trim_end_matches('/'), context.row_name);
        context.row_in_dropbox = dropbox::contains(&account.path, &row);
    }

    // The Dropbox root a dropbox or sharelink validate names, so the backend checks the same folder.
    pub(crate) fn provider_destination(&self, side: usize) -> String {
        self.menu.providers.account[side].path.clone()
    }

    // A provider answer under an open menu rebuilds its rows in place; before the card shows, the load path does.
    fn provider_refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.menu_opened() {
            self.refresh_menu(window, cx);
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
    }

    fn observe_providers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.providers.chosen.is_some() {
            return;
        }
        let entity = cx.entity();
        self.menu.providers.chosen = Some(cx.subscribe_in(
            &entity,
            window,
            |shell, _, chosen: &crate::g2::mn1::join::Chosen, window, cx| {
                shell.provider_chosen(chosen, window, cx);
            },
        ));
    }

    // A checked provider row: claim it, dispatch through this side's sender, then close the snapshot.
    pub(crate) fn provider_chosen(
        &mut self,
        chosen: &crate::g2::mn1::join::Chosen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = chosen.captured.side;
        if !matches!(chosen.action.as_str(), "localsend" | "taildrop" | "sharelink" | "dropbox")
            || chosen.captured.background
            || self.panes.get(side).and_then(Option::as_ref).is_none()
            || !chosen.claim()
        {
            return;
        }
        let catalog = self.locale.catalog.clone();
        let id = chosen.captured.id;
        let actions = match chosen.action.as_str() {
            "localsend" => {
                let backend = self.panes[side].as_ref().and_then(|host| host.backend.as_ref());
                self.menu.providers.sides[side].send_localsend(&Pipe(backend), &chosen.leaf, &chosen.paths, &catalog)
            }
            "taildrop" => self.menu.providers.sides[side].send_taildrop(&chosen.leaf, &chosen.paths, &catalog),
            "sharelink" => {
                let path = if chosen.paths.len() == 1 { chosen.paths[0].as_str() } else { "" };
                self.menu.providers.sides[side].copy_share_link(path, &catalog)
            }
            _ => {
                let dest = self.menu.providers.account[side].path.clone();
                if self.menu.providers.destination[side] == Some(true) && !dest.is_empty() {
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
        self.menu_send(side, format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{id}}}"), cx);
        self.provider_actions(side, actions, window, cx);
    }

    // Says go to the footer; runs leave the UI thread and answer back on this side's sender.
    fn provider_actions(&mut self, side: usize, actions: Vec<Action>, window: &mut Window, cx: &mut Context<Self>) {
        for action in actions {
            match action {
                Action::Say(text, error) => {
                    self.status_messages(cx, |messages, now| {
                        messages.say(&text, error, "", flea::ui_logic_fb1::status::Hint::None, now);
                    });
                }
                // A send may run for minutes, so it waits on its own thread, never an executor worker.
                Action::Spawn(argv) => {
                    std::thread::spawn(move || {
                        command(&argv)
                            .stdin(Stdio::null())
                            .stdout(Stdio::null())
                            .stderr(Stdio::null())
                            .status()
                    });
                }
                Action::Run(kind, argv) => self.provider_run(side, kind, argv, window, cx),
            }
        }
        cx.notify();
    }

    fn provider_run(&mut self, side: usize, kind: RunKind, argv: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        let kind_is_clipboard = kind == RunKind::Clipboard;
        cx.spawn_in(window, async move |this, cx| {
            let output = cx
                .background_executor()
                .spawn(async move {
                    let mut command = command(&argv);
                    command.stdin(Stdio::null());
                    // wl-copy leaves a server holding its stdout, so only its exit status is read.
                    if kind_is_clipboard {
                        command.stdout(Stdio::null()).stderr(Stdio::null());
                    }
                    command.output()
                })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let catalog = shell.locale.catalog.clone();
                let sender = &mut shell.menu.providers.sides[side];
                let (started, code, stdout, stderr) = match &output {
                    Ok(out) => (
                        true,
                        exit_code(&out.status),
                        String::from_utf8_lossy(&out.stdout).into_owned(),
                        String::from_utf8_lossy(&out.stderr).into_owned(),
                    ),
                    Err(error) => (false, -1, String::new(), error.to_string()),
                };
                let actions = match kind {
                    RunKind::TaildropStatus(generation) => {
                        sender.taildrop_done(generation, code, &stdout, &stderr, &catalog)
                    }
                    RunKind::ShareLink => sender.sharelink_done(started, code, &stdout, &catalog),
                    RunKind::Clipboard => sender.clipboard_done(started && code == 0, &catalog),
                };
                shell.provider_actions(side, actions, window, cx);
                if matches!(kind, RunKind::TaildropStatus(_)) {
                    shell.provider_refresh(window, cx);
                }
            });
        })
        .detach();
    }
}

fn command(argv: &[String]) -> Command {
    let mut command = Command::new(argv.first().map_or("", String::as_str));
    command.args(argv.iter().skip(1));
    command
}

// A signal death reads as 128 plus the signal, as a shell and ui/Taildrop.qml's exit code do.
fn exit_code(status: &std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status.code().or_else(|| status.signal().map(|signal| 128 + signal)).unwrap_or(-1)
}
