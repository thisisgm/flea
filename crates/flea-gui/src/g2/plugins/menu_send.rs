//! Built-in transports and external targets enter through their declared send contribution.
use crate::{
    g2::{
        context_menus::join::{Captured, Chosen},
        send_share::seams::ProviderState,
    },
    state::Shell,
};
use flea::{
    jsondoc::Json,
    ui_logic_context_menus::{entry::Text, Entry},
    ui_logic_custom_actions::rows,
    ui_logic_plugin_settings::wire::Source,
    ui_logic_send_share::providers::Role,
};
use gpui::{Context, Window};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

const PREFIX: &str = "plugin-send:";
fn builtin(action: &str) -> Option<Role> {
    match action {
        "taildrop" => Some(Role::Tailscale),
        "localsend" => Some(Role::LocalSend),
        "dropbox" | "sharelink" => Some(Role::Dropbox),
        "airdrop" => Some(Role::AirDrop),
        _ => None,
    }
}

/// The built-in provider a send row names, by its own verb or by the plugin-send action it is rewritten to.
pub(crate) fn send_role(action: &str) -> Option<Role> {
    builtin(action).or_else(|| {
        action.strip_prefix(PREFIX).and_then(|id| {
            [
                Role::Tailscale,
                Role::LocalSend,
                Role::Dropbox,
                Role::AirDrop,
            ]
            .into_iter()
            .find(|r| r.plugin_id() == id)
        })
    })
}

impl Shell {
    pub(crate) fn plugin_provider_states(&mut self) {
        let states = self.declared_provider_states();
        let hidden = self.menu_hidden();
        for sender in &mut self.menu.providers.sides {
            sender.states_changed(Some(&states), &hidden);
        }
    }

    pub(crate) fn declared_provider_states(&self) -> Vec<ProviderState> {
        [
            Role::Tailscale,
            Role::LocalSend,
            Role::Dropbox,
            Role::AirDrop,
        ]
        .iter()
        .map(|role| ProviderState {
            plugin: role.plugin_id().into(),
            enabled: self
                .declared_menu_plugins("send-target")
                .any(|p| p.manifest.id == role.plugin_id()),
            failed: false,
        })
        .collect()
    }

    // Sample input, one plugin's stored answer: {"op":"targets","targets":[{"id":"receiver","label":"Local Receiver","state":"ready"}]}
    pub(crate) fn plugin_send_rows(
        &self,
        entries: &mut Vec<Entry>,
        background: bool,
        hidden: &[String],
    ) {
        if background {
            return;
        }
        for row in entries.iter_mut() {
            if let Some(role) = builtin(&row.action) {
                // The generic validation verb returns the selected files, including a marked set whose cursor is outside it.
                if matches!(role, Role::Tailscale | Role::AirDrop) {
                    row.action = format!("{PREFIX}{}", role.plugin_id()).into();
                }
            }
        }
        let rows = self
            .declared_menu_plugins("send-target")
            .filter(|p| p.source != Source::Builtin)
            .filter(|p| {
                !hidden
                    .iter()
                    .any(|id| id == &p.manifest.id || id == &format!("{PREFIX}{}", p.manifest.id))
            })
            .map(|p| {
                let block = p.manifest.block("send-target").expect("declared target");
                let action = format!("{PREFIX}{}", p.manifest.id);
                let mut row = Entry::leaf(
                    action.clone(),
                    Text::Data(
                        block
                            .get("label")
                            .and_then(Json::as_str)
                            .unwrap_or(&p.manifest.name)
                            .into(),
                    ),
                );
                row.action = action.clone().into();
                row.glyph = rows::static_glyph(
                    block
                        .get("glyph")
                        .and_then(Json::as_str)
                        .unwrap_or("network"),
                );
                let targets = self
                    .menu
                    .plugin_runtime
                    .targets
                    .get(&p.manifest.id)
                    .and_then(|a| a.get("targets"))
                    .and_then(Json::as_array)
                    .unwrap_or(&[]);
                let leaves = targets
                    .iter()
                    .filter(|target| {
                        ["id", "label", "state"].iter().all(|key| {
                            target.get(key).and_then(Json::as_str).is_some_and(|text| {
                                !text.is_empty()
                                    && text.chars().count()
                                        <= crate::g2::plugins::limits::TARGET_TEXT
                                    && !text.chars().any(char::is_control)
                            })
                        })
                    })
                    .map(|target| {
                        let mut leaf = Entry::leaf(
                            target
                                .get("id")
                                .and_then(Json::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            Text::Data(
                                target
                                    .get("label")
                                    .and_then(Json::as_str)
                                    .unwrap_or("")
                                    .into(),
                            ),
                        );
                        leaf.disabled = target.get("state").and_then(Json::as_str) != Some("ready");
                        leaf.errored = leaf.disabled;
                        leaf
                    })
                    .collect::<Vec<_>>();
                row.disabled = !leaves.iter().any(|leaf| !leaf.disabled);
                row.errored = row.disabled;
                row.submenu = Some(leaves);
                row
            })
            .collect::<Vec<_>>();
        super::layout::insert_sends(entries, rows);
    }

    pub(super) fn ask_plugin_targets(&mut self, cx: &mut Context<Self>) {
        if self.menu.plugin_runtime.input.is_some()
            || self.menu.captured.as_ref().is_none_or(|c| c.background)
        {
            return;
        }
        let Some(host) = self.plugins.host.clone() else {
            return;
        };
        let now = Instant::now();
        let wanted = self
            .declared_menu_plugins("send-target")
            .filter(|p| p.source != Source::Builtin)
            .filter(|p| {
                !self
                    .menu
                    .plugin_runtime
                    .target_pending
                    .contains(&p.manifest.id)
            })
            .filter(|p| {
                self.menu
                    .plugin_runtime
                    .target_due
                    .get(&p.manifest.id)
                    .is_none_or(|due| now >= *due)
            })
            .map(|p| {
                (
                    p.manifest.id.clone(),
                    p.manifest
                        .block("send-target")
                        .and_then(|b| b.get("refreshSec"))
                        .and_then(Json::as_f64)
                        .filter(|v| v.is_finite())
                        .unwrap_or(crate::g2::plugins::limits::REFRESH_SECONDS)
                        .clamp(
                            crate::g2::plugins::limits::REFRESH_RANGE.0,
                            crate::g2::plugins::limits::REFRESH_RANGE.1,
                        ),
                )
            })
            .collect::<Vec<_>>();
        for (plugin, seconds) in wanted {
            let sent = host.update(cx, |host, cx| {
                host.request(
                    &plugin,
                    Json::Obj(vec![("op".into(), Json::Str("targets".into()))]),
                    cx,
                )
            });
            if sent.is_some() {
                self.menu
                    .plugin_runtime
                    .target_pending
                    .insert(plugin.clone());
                self.menu
                    .plugin_runtime
                    .target_due
                    .insert(plugin, now + Duration::from_secs_f64(seconds));
            }
        }
    }

    pub(crate) fn deliver_plugin_send(
        &mut self,
        capture: &Captured,
        action: &str,
        leaf: &str,
        paths: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let role = send_role(action);
        let plugin = action
            .strip_prefix(PREFIX)
            .map(str::to_owned)
            .or_else(|| role.map(|r| r.plugin_id().into()));
        let Some(plugin) = plugin else {
            return false;
        };
        if capture.background
            || paths.is_empty()
            || !self
                .declared_menu_plugins("send-target")
                .any(|p| p.manifest.id == plugin)
        {
            self.menu_notice(
                capture.side,
                self.locale.catalog.tr("menu.refusal.unavailable"),
                cx,
            );
            return true;
        }
        let request = Json::Obj(vec![
            ("op".into(), Json::Str("send".into())),
            ("target".into(), Json::Str(leaf.into())),
            (
                "paths".into(),
                Json::Arr(paths.iter().cloned().map(Json::Str).collect()),
            ),
        ]);
        if let Some(role) = role {
            let action = if action == "sharelink" {
                action
            } else {
                role.menu_id()
            };
            let chosen = Chosen {
                captured: capture.clone(),
                action: action.into(),
                leaf: leaf.into(),
                paths: request
                    .get("paths")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_owned)
                    .collect(),
                claimed: Rc::new(Cell::new(false)),
            };
            self.provider_chosen(&chosen, window, cx);
        } else {
            self.plugin_selection_request(capture.side, &plugin, request, cx);
        }
        true
    }
}
