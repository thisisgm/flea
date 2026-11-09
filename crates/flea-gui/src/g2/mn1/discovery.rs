//! Menu discovery reuses the backend's bounded Actions and application registry producers.
use super::delivery::string;
use crate::state::Shell;
use flea::{json::escape, jsondoc::Json, ui_logic_mn1::build::Peer};
use gpui::{Context, Window};

/// How long a menu waits on the applications catalogue before it draws without it.
pub(crate) const APPLICATIONS_WAIT_MS: u64 = 400;

impl Shell {
    pub(crate) fn discover_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(capture) = self.menu.captured.clone() else {
            return;
        };
        let hidden = self
            .launch
            .settings
            .data
            .get("menu")
            .and_then(|menu| menu.get("hidden"))
            .and_then(Json::as_array);
        let hidden_scripts = hidden.map_or_else(
            || flea::ui_logic_mn1::inventory::SHIPPED_HIDDEN.contains(&"runScript"),
            |hidden| hidden.iter().any(|id| id.as_str() == Some("runScript")),
        );
        if hidden_scripts {
            self.discover_menu_apps(window, cx);
        } else {
            self.menu.reading = true;
            self.menu_send(capture.side, format!("{{\"c\":\"menuaction\",\"op\":\"validate\",\"id\":{},\"action\":\"__menuScripts\"}}", capture.id), cx);
        }
    }

    fn discover_menu_apps(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(capture) = self
            .menu
            .captured
            .clone()
            .filter(|c| !c.rows.is_empty() && !c.background && self.menu.pending.is_none())
        else {
            self.menu.context.open_with_loaded = true;
            self.refresh_menu(window, cx);
            return;
        };
        self.menu.reading = true;
        self.menu_send(
            capture.side,
            format!(
                "{{\"c\":\"menuaction\",\"op\":\"applications\",\"id\":{}}}",
                capture.id
            ),
            cx,
        );
        // A hung catalogue opens the menu after the deadline with Open With kept for Another Application; delivery.rs fills it from a late answer.
        let id = capture.id;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(APPLICATIONS_WAIT_MS))
                .await;
            let _ = cx.update(|window, cx| {
                this.update(cx, |shell, cx| {
                    let waiting = shell.menu.reading
                        && !shell.menu.context.open_with_loaded
                        && shell.menu.captured.as_ref().is_some_and(|c| c.id == id);
                    if waiting {
                        shell.menu.reading = false;
                        shell.menu.context.open_with_loaded = true;
                        shell.menu.context.open_with_late = true;
                        shell.menu.context.open_with_apps.clear();
                        shell.refresh_menu(window, cx);
                    }
                })
            });
        })
        .detach();
    }

    pub(crate) fn menu_discovery_reply(
        &mut self,
        side: usize,
        line: &str,
        reply: &Json,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let scripts = string(reply, "t") == "actions";
        let validation = string(reply, "t") == "menuaction"
            && string(reply, "op") == "validate"
            && string(reply, "action") == "__menuScripts";
        if !scripts && !validation {
            return false;
        }
        let id = match reply.get("id") {
            Some(Json::Num(value)) => value.parse::<u64>().unwrap_or(0),
            _ => 0,
        };
        let Some(capture) = self
            .menu
            .captured
            .clone()
            .filter(|c| c.id == id && c.side == side && self.menu.reading)
        else {
            return false;
        };
        if capture.identity != self.menu_identity() {
            self.menu_refuse("menu.refusal.moved", cx);
            self.retire_menu(window, cx);
            return true;
        }
        if validation {
            if reply.get("ok").and_then(Json::as_bool) != Some(true) {
                self.menu_refuse("menu.refusal.moved", cx);
                self.retire_menu(window, cx);
                return true;
            }
            let paths = reply
                .get("paths")
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter_map(Json::as_str)
                .map(|path| format!("\"{}\"", escape(path)))
                .collect::<Vec<_>>()
                .join(",");
            self.menu.actions.listing_requested(id as usize);
            self.menu_send(
                side,
                format!("{{\"c\":\"actions\",\"op\":\"list\",\"id\":{id},\"paths\":[{paths}]}}"),
                cx,
            );
        } else {
            self.menu.reading = false;
            self.menu.actions.receive(line);
            self.actions_said(side, cx);
            self.menu.context.scripts = reply
                .get("actions")
                .and_then(Json::as_array)
                .unwrap_or(&[])
                .iter()
                .filter(|action| action.get("script").and_then(Json::as_bool) == Some(true))
                .map(|action| Peer {
                    id: string(action, "id").into(),
                    label: string(action, "name").into(),
                })
                .collect();
            if self.menu.pending.is_some() {
                self.activate_menu(window, cx);
            } else {
                self.discover_menu_apps(window, cx);
            }
        }
        true
    }
}
