//! Declared menu contributions share the captured selection and the supervised plugin host.
use super::Response;
use crate::state::Shell;
use flea::{
    jsondoc::Json,
    ui_logic_context_menus::Entry,
    ui_logic_custom_actions::{plugins, rows},
    ui_logic_plugin_settings::{contributions, store, wire::Plugin},
};
use gpui::{Context, Subscription, Task, Window};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, OnceLock},
    time::Instant,
};

#[path = "menu_layout.rs"]
mod layout;
#[path = "../../../../../src/backend/mime.rs"]
mod mime;
pub(crate) use layout::insert_actions;
#[path = "menu_action.rs"]
mod action;
#[path = "menu_input.rs"]
mod prompt;
#[path = "menu_selection.rs"]
mod selection;
#[path = "menu_send.rs"]
mod send;
#[path = "menu_verdict.rs"]
mod verdict;
pub(crate) use send::{send_role, send_validation_verb, validation_choice};
#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;

// A test's stand-in for the owner: plugin id and request in, answer out.
#[cfg(test)]
type Dispatch = Box<dyn FnMut(&str, Json) -> Result<Json, String>>;

#[derive(Default)]
pub(crate) struct Menus {
    pub(super) plugins: Vec<Plugin>,
    pub(super) targets: HashMap<String, Json>,
    pub(super) pending: HashMap<u64, usize>,
    errors: HashMap<verdict::Owner, Vec<u64>>,
    attempts: verdict::Attempts,
    pub(super) target_pending: HashSet<String>,
    pub(super) target_due: HashMap<String, Instant>,
    observers: Vec<Subscription>,
    mimes: Option<Arc<mime::Db>>,
    loading: Option<Task<()>>,
    refresh: Option<Task<()>>,
    input: Option<prompt::Pending>,
    input_task: Option<Task<()>>,
    selection: Option<selection::Selection>,
    #[cfg(test)]
    dispatch: Option<Dispatch>,
}

impl Shell {
    pub(crate) fn declared_menu_plugins<'a>(
        &'a self,
        kind: &'a str,
    ) -> impl Iterator<Item = &'a Plugin> + 'a {
        self.menu.plugin_runtime.plugins.iter().filter(move |p| {
            p.enabled
                && p.failure.is_none()
                && p.manifest.block(kind).is_some()
                && store::enabled(&self.launch.settings.data, &p.manifest.id, p.enabled)
        })
    }

    pub(crate) fn plugin_menu_items(
        &self,
        side: usize,
        background: bool,
    ) -> Option<Vec<plugins::Item>> {
        if background {
            return Some(Vec::new());
        }
        let capture = self
            .menu
            .captured
            .as_ref()
            .filter(|c| c.side == side && !c.background)?;
        let host = self.panes.get(side)?.as_ref()?;
        if capture.identity != self.menu_identity() || capture.rows.len() > super::limits::SELECTION
        {
            return None;
        }
        if let Some(items) = self.captured_plugin_items(side, capture.id) {
            return Some(items);
        }
        capture
            .rows
            .iter()
            .map(|&index| {
                let row = host.pane.held.row(index)?;
                Some(plugins::Item {
                    folder: row.dir,
                    mime: if row.dir {
                        "inode/directory".into()
                    } else {
                        self.menu
                            .plugin_runtime
                            .mimes
                            .as_ref()
                            .and_then(|db| db.lookup(&row.name))
                            .unwrap_or("application/octet-stream")
                            .into()
                    },
                })
            })
            .collect()
    }

    pub(crate) fn plugin_action_rows(
        &self,
        side: usize,
        background: bool,
        hidden: &[String],
    ) -> Vec<Entry> {
        let Some(items) = self.plugin_menu_items(side, background) else {
            return Vec::new();
        };
        let contributions: Vec<_> = self
            .declared_menu_plugins("menu-action")
            .flat_map(|p| contributions::menu_actions(&p.manifest).unwrap_or_default())
            .collect();
        let shown = plugins::eligible(&contributions, background, &items)
            .into_iter()
            .filter(|c| {
                c.when.mime.is_empty() || background || self.menu.plugin_runtime.mimes.is_some()
            })
            .filter(|c| {
                !hidden
                    .iter()
                    .any(|id| id == &c.plugin || id == &plugins::leaf(c))
            })
            .cloned()
            .collect::<Vec<_>>();
        rows::plugin_rows(&shown)
    }

    pub(crate) fn prepare_plugin_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.follow_plugins(window, cx);
        let Some(host) = self.plugins.host.clone() else {
            return;
        };
        if self.menu.plugin_runtime.observers.is_empty() {
            self.menu.plugin_runtime.observers.push(cx.observe_in(
                &host,
                window,
                |shell, host, window, cx| {
                    let host = host.read(cx);
                    shell.menu.plugin_runtime.plugins = host
                        .plugins
                        .iter()
                        .filter(|p| host.healthy(p))
                        .cloned()
                        .collect();
                    shell.menu.plugin_runtime.targets = host
                        .answers
                        .iter()
                        .filter(|((_, op), _)| op == "targets")
                        .map(|((id, _), answer)| (id.clone(), answer.clone()))
                        .collect();
                    shell.plugin_provider_states();
                    shell.airdrop_plugin_off(window, cx);
                    shell.ask_plugin_targets(cx);
                    shell.provider_refresh(window, cx);
                },
            ));
            self.menu.plugin_runtime.observers.push(cx.subscribe_in(
                &host,
                window,
                |shell, host, line: &super::WireLine, _, cx| {
                    if matches!(
                        flea::ui_logic_plugin_settings::wire::parse(&line.0),
                        Ok(flea::ui_logic_plugin_settings::wire::Reply::Listing(_))
                    ) && shell.menu.captured.is_some()
                        && !shell.menu.header
                    {
                        host.update(cx, |host, cx| {
                            host.prepare("menu-action", cx);
                            if shell
                                .menu
                                .captured
                                .as_ref()
                                .is_some_and(|capture| !capture.background)
                            {
                                host.prepare("send-target", cx);
                                host.prepare("shelf", cx);
                            }
                        });
                    }
                },
            ));
            self.menu.plugin_runtime.observers.push(cx.subscribe_in(
                &host,
                window,
                |shell, _, response: &Response, _, cx| {
                    let terminal =
                        response.message.get("op").and_then(Json::as_str) != Some("progress");
                    if terminal
                        && response.request.get("op").and_then(Json::as_str) == Some("targets")
                    {
                        shell
                            .menu
                            .plugin_runtime
                            .target_pending
                            .remove(&response.plugin);
                    }
                    if let Some(side) = shell.menu.plugin_runtime.pending.get(&response.id).copied()
                    {
                        let current = shell.plugin_action_response(side, response, terminal);
                        if terminal {
                            shell.menu.plugin_runtime.pending.remove(&response.id);
                        }
                        if !current {
                            return;
                        }
                        let said = if response.ok {
                            response
                                .message
                                .get("message")
                                .and_then(Json::as_str)
                                .unwrap_or("")
                        } else {
                            &response.error
                        };
                        if !response.ok || !said.is_empty() {
                            let said: String = said
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(super::limits::TEXT)
                                .collect();
                            let sentence = if response.ok {
                                if said.is_empty() {
                                    return;
                                }
                                said
                            } else if said.trim().is_empty() {
                                shell.locale.catalog.tr("feedback.error.generic")
                            } else {
                                shell
                                    .locale
                                    .catalog
                                    .tr_args("feedback.error.backend", &[&said])
                            };
                            shell.plugin_response_notice(side, response, &sentence, cx);
                        }
                    }
                },
            ));
        }
        {
            self.menu.plugin_runtime.plugins = host
                .read(cx)
                .plugins
                .iter()
                .filter(|p| host.read(cx).healthy(p))
                .cloned()
                .collect();
            self.menu.plugin_runtime.targets = host
                .read(cx)
                .answers
                .iter()
                .filter(|((_, op), _)| op == "targets")
                .map(|((id, _), answer)| (id.clone(), answer.clone()))
                .collect();
        }
        host.update(cx, |host, cx| {
            host.prepare("menu-action", cx);
            if self
                .menu
                .captured
                .as_ref()
                .is_some_and(|capture| !capture.background)
            {
                host.prepare("send-target", cx);
                host.prepare("shelf", cx);
            }
        });
        self.menu.plugin_runtime.target_due.clear();
        self.ask_plugin_targets(cx);
        self.load_menu_mimes(window, cx);
        let id = self.menu.captured.as_ref().map(|c| c.id);
        self.menu.plugin_runtime.refresh = Some(cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(super::limits::RETRY).await;
            let active = this
                .update_in(cx, |shell, _, cx| {
                    if shell.menu.captured.as_ref().map(|c| c.id) != id {
                        return false;
                    }
                    shell.ask_plugin_targets(cx);
                    true
                })
                .unwrap_or(false);
            if !active {
                break;
            }
        }));
    }

    pub(super) fn pause_plugin_menu(&mut self) {
        self.menu.plugin_runtime.refresh = None;
        self.menu.plugin_runtime.target_due.clear();
        for sender in &mut self.menu.providers.sides {
            sender.menu_closed();
        }
    }

    pub(crate) fn retire_plugin_menu(&mut self) {
        self.pause_plugin_menu();
        self.menu.plugin_runtime.input = None;
        self.menu.plugin_runtime.input_task = None;
        self.menu.plugin_runtime.selection = None;
    }

    pub(crate) fn menu_mimes_loaded(&self) -> bool {
        self.menu.plugin_runtime.mimes.is_some()
    }

    pub(crate) fn load_menu_mimes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.plugin_runtime.mimes.is_some() || self.menu.plugin_runtime.loading.is_some() {
            return;
        }
        static DB: OnceLock<Arc<mime::Db>> = OnceLock::new();
        self.menu.plugin_runtime.loading = Some(cx.spawn_in(window, async move |this, cx| {
            let db = cx
                .background_executor()
                .spawn(async { DB.get_or_init(|| Arc::new(mime::Db::load())).clone() })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.menu.plugin_runtime.mimes = Some(db);
                shell.menu.plugin_runtime.loading = None;
                shell.provider_refresh(window, cx);
            });
        }));
    }
}
