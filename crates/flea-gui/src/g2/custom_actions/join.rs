//! The window's side of Open in Agent and the Actions flyout: the menu join delivers their choices here, the A key opens the agent with no menu, and Actions replies are read back into the menu's own model.
use super::model::{Said, Sent};
use crate::{g2::context_menus::join::Captured, state::Shell};
use flea::jsondoc::{self, Json};
use gpui::{Context, Window};
use std::path::Path;

impl Shell {
    // A captured menu choice mn2 owns: Open in Agent in the captured folder, or an Actions leaf over the validated selection.
    pub(crate) fn deliver_actions(
        &mut self,
        capture: &Captured,
        action: &str,
        leaf: &str,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.deliver_plugin_action(capture, action, &paths, window, cx) {
            return true;
        }
        if self.deliver_plugin_send(capture, action, leaf, &paths, window, cx) {
            self.retire_menu(window, cx);
            return true;
        }
        match action {
            flea::ui_logic_custom_actions::agent::ID => {
                self.retire_menu(window, cx);
                self.open_agent(capture.side, &capture.path, cx);
                true
            }
            flea::ui_logic_custom_actions::rows::FIRE
                if !capture.background && !leaf.is_empty() =>
            {
                let cursor = self.panes[capture.side]
                    .as_ref()
                    .and_then(|host| capture.row(host, capture.cursor))
                    .map(|row| Path::new(&capture.path).join(&row.name))
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let sent = self
                    .menu
                    .actions
                    .choose_on(capture.side, leaf, paths, cursor);
                if let Some(Sent::Run(line) | Sent::Preview(line)) = sent {
                    self.menu_send(capture.side, line, cx);
                }
                self.retire_menu(window, cx);
                self.actions_said(capture.side, cx);
                true
            }
            _ => false,
        }
    }

    // The A key: Open in Agent in the focused pane's folder, with no menu and a quiet status bar.
    pub(crate) fn agent_key(&mut self, cx: &mut Context<Self>) {
        let side = self.focus_side;
        let Some(folder) = self.panes[side]
            .as_ref()
            .filter(|host| !host.stopped && host.path.starts_with('/'))
            .map(|host| host.path.clone())
        else {
            return;
        };
        self.open_agent(side, &folder, cx);
    }

    // The detached spawn, reaped off the UI thread; only a failure to start says anything.
    fn open_agent(&mut self, side: usize, folder: &str, cx: &mut Context<Self>) {
        if let Some(launch) = self.menu.agent.clone() {
            self.agent_launched(side, launch(Path::new(folder)), cx);
            return;
        }
        let folder = Path::new(folder).to_owned();
        let launched = cx
            .background_executor()
            .spawn(async move { super::launch::launch_agent_reaped(&folder) });
        cx.spawn(async move |this, cx| {
            let result = launched.await;
            let _ = this.update(cx, |shell, cx| shell.agent_launched(side, result, cx));
        })
        .detach();
    }

    fn agent_launched(&mut self, side: usize, result: std::io::Result<()>, cx: &mut Context<Self>) {
        if let Err(error) = result {
            let sentence = self
                .locale
                .catalog
                .tr_args(super::model::START_KEY, &[&error.to_string()]);
            self.menu_notice(side, sentence, cx);
        }
    }

    // Run and preview replies belong to the menu's Actions model; Jobs lines after the handshake stay with OP1.
    pub(crate) fn actions_reply(
        &mut self,
        side: usize,
        line: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(reply) = jsondoc::parse(line) else {
            return false;
        };
        // A Jobs line stays OP1's; the model only reads whether a held script's job failed.
        if reply.get("t").and_then(Json::as_str) == Some("job") {
            self.menu.actions.receive_on(side, line);
            self.actions_said(side, cx);
            return false;
        }
        if !matches!(
            reply.get("t").and_then(Json::as_str),
            Some("actionstarted" | "actionpreview")
        ) {
            return false;
        }
        self.menu.actions.receive_on(side, line);
        self.actions_said(side, cx);
        true
    }

    // Every sentence the model holds reaches the status bar once, through its catalog key.
    pub(crate) fn actions_said(&mut self, side: usize, cx: &mut Context<Self>) {
        for said in self.menu.actions.take_said() {
            let sentence = match said {
                Said::Key { key, args } => {
                    let args: Vec<&str> = args.iter().map(String::as_str).collect();
                    self.locale.catalog.tr_args(key, &args)
                }
                Said::Line(line) => line,
            };
            self.menu_notice(side, sentence, cx);
        }
    }
}
