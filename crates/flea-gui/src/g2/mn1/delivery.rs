//! Snapshot retirement, backend validation and captured choices delivered to their producer owners.
use super::{join::Chosen, menu::Outcome};
use crate::{
    backend::Failure,
    state::Shell,
    window::{dispatch::Order, navigate::Step},
};
use flea::{
    json::escape,
    jsondoc::Json,
    ui_logic_mn1::{build::App, refresh},
};
use gpui::{Context, Window};

pub(crate) fn string<'a>(value: &'a Json, key: &str) -> &'a str {
    value.get(key).and_then(Json::as_str).unwrap_or("")
}

impl Shell {
    pub(crate) fn menu_send(&mut self, side: usize, line: String, cx: &mut Context<Self>) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let result = host
            .backend
            .as_ref()
            .ok_or_else(|| Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            })
            .and_then(|backend| backend.line(line));
        if let Err(error) = result {
            host.record_error(error, &self.locale);
            self.menu.pending = None;
            self.menu.activating = false;
            self.menu.reading = false;
            self.menu.menu.borrow_mut().close();
            self.menu.captured = None;
            self.menu.presentation.cancel();
            self.keyboard.context = flea::keys::Context::Listing;
            host.nav.gate.menu_open = false;
            host.nav.gate.menu_pending = false;
            let sentence = host.status.to_string();
            self.menu_notice(side, sentence, cx);
            cx.notify();
        }
    }

    pub(crate) fn menu_refuse(&mut self, key: &str, cx: &mut Context<Self>) {
        let side = self
            .menu
            .captured
            .as_ref()
            .map_or(self.focus_side, |c| c.side);
        self.menu_notice(side, self.locale.catalog.tr(key), cx);
    }

    pub(crate) fn menu_notice(&mut self, side: usize, sentence: String, cx: &mut Context<Self>) {
        if let Some(host) = &mut self.panes[side] {
            host.status = sentence.clone().into();
        }
        self.status_messages(cx, |messages, now| {
            messages.say(
                &sentence,
                true,
                "",
                flea::ui_logic_fb1::status::Hint::None,
                now,
            );
        });
        cx.notify();
    }

    pub(crate) fn retire_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.providers_menu_closed();
        self.menu.menu.borrow_mut().close();
        self.menu.presentation.cancel();
        self.menu.pending = None;
        self.menu.activating = false;
        self.menu.reading = false;
        if let Some(capture) = self.menu.captured.take() {
            if !capture.background {
                self.menu_send(
                    capture.side,
                    format!(
                        "{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{}}}",
                        capture.id
                    ),
                    cx,
                );
            }
            if let Some(host) = &self.panes[capture.side] {
                let gate = crate::navigation::watch::Gate {
                    menu_open: false,
                    menu_pending: false,
                    ..host.nav.gate
                };
                self.set_gate(capture.side, gate, window, cx);
            }
        }
        self.keyboard.context = flea::keys::Context::Listing;
        self.keyboard.disarm();
        cx.notify();
    }

    pub(crate) fn activate_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(capture) = self.menu.captured.clone() else {
            return;
        };
        let Some((action, leaf)) = self.menu.pending.clone() else {
            return;
        };
        if self.menu.activating || self.menu.reading {
            return;
        }
        if capture.identity != self.menu_identity()
            || self.panes[capture.side]
                .as_ref()
                .is_none_or(|host| host.pane.busy() || host.stopped)
        {
            self.menu_outcome(Outcome::Refused("menu.refusal.moved"), window, cx);
            return;
        }
        if refresh::validate_choice(&self.menu_entries(), &action, &leaf, false).is_err() {
            self.menu_outcome(Outcome::Refused("menu.refusal.unavailable"), window, cx);
            return;
        }
        if capture.background {
            self.deliver_menu(Vec::new(), window, cx);
        } else if capture.ready {
            self.menu.activating = true;
            if let Some(host) = &mut self.panes[capture.side] {
                host.nav.gate.menu_pending = true;
            }
            // Dropbox rows name the account root the backend already took as this menu's destination.
            let dest = if matches!(action.as_str(), "dropbox" | "sharelink") {
                format!(",\"dest\":\"{}\"", escape(&self.provider_destination(capture.side)))
            } else {
                String::new()
            };
            let chosen = if leaf.is_empty() {
                action
            } else {
                format!("{action}:{leaf}")
            };
            self.menu_send(
                capture.side,
                format!(
                    "{{\"c\":\"menuaction\",\"op\":\"validate\",\"id\":{},\"action\":\"{}\"{dest}}}",
                    capture.id,
                    escape(&chosen)
                ),
                cx,
            );
        }
    }

    /// Menu producer replies retain their own ids; a dismissed or replaced menu cannot be resurrected.
    pub(crate) fn menu_reply(
        &mut self,
        side: usize,
        line: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // The archive and flash host answers its own lines first, so none is forwarded as an unclaimed reply.
        if self.archive_line(side, line, window, cx) {
            return true;
        }
        let Ok(reply) = flea::jsondoc::parse(line) else {
            return false;
        };
        if self.actions_reply(side, line, cx) {
            return true;
        }
        if self.menu_discovery_reply(side, line, &reply, window, cx) {
            return true;
        }
        if string(&reply, "t") != "menuaction" {
            return false;
        }
        let id = flea::json::field_usize(line, "id").unwrap_or(0) as u64;
        let Some(capture) = self
            .menu
            .captured
            .clone()
            .filter(|c| c.id == id && c.side == side)
        else {
            return false;
        };
        let operation = string(&reply, "op");
        if !matches!(operation, "snapshot" | "validate" | "applications") {
            return false;
        }
        if capture.identity != self.menu_identity() {
            self.menu_outcome(Outcome::Refused("menu.refusal.moved"), window, cx);
            return true;
        }
        if reply.get("ok").and_then(Json::as_bool) != Some(true) {
            if operation == "applications" {
                self.menu.reading = false;
                self.menu.context.open_with_loaded = true;
                self.menu.context.open_with_apps.clear();
                self.refresh_menu(window, cx);
                self.activate_menu(window, cx);
            } else {
                self.menu_outcome(Outcome::Refused("menu.refusal.moved"), window, cx);
            }
            return true;
        }
        match operation {
            "snapshot" => {
                let capture = self.menu.captured.as_mut().expect("current snapshot");
                capture.ready = true;
                capture.kinds = reply
                    .get("kinds")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|kind| match kind {
                        Json::Num(value) => value.parse::<u32>().ok(),
                        _ => None,
                    })
                    .collect();
                self.menu.context.compare_kinds = Some(capture.kinds.clone());
                if self.menu.pending.is_some() {
                    self.activate_menu(window, cx);
                } else {
                    self.discover_menu(window, cx);
                }
            }
            "applications" => {
                self.menu.reading = false;
                self.menu.context.open_with_loaded = true;
                self.menu.context.open_with_apps = reply
                    .get("applications")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .map(|app| App {
                        id: string(app, "id").into(),
                        label: string(app, "label").into(),
                        icon: string(app, "icon").into(),
                        default: app.get("default").and_then(Json::as_bool) == Some(true),
                    })
                    .collect();
                self.refresh_menu(window, cx);
                self.activate_menu(window, cx);
            }
            "validate" if self.menu.activating => {
                let Some((action, leaf)) = &self.menu.pending else {
                    return true;
                };
                let expected = if leaf.is_empty() {
                    action.clone()
                } else {
                    format!("{action}:{leaf}")
                };
                if string(&reply, "action") != expected {
                    return true;
                }
                let paths = reply
                    .get("paths")
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(Json::as_str)
                    .map(str::to_owned)
                    .collect();
                if refresh::validate_choice(&self.menu_entries(), action, leaf, false).is_err() {
                    self.menu_outcome(Outcome::Refused("menu.refusal.unavailable"), window, cx);
                } else {
                    self.deliver_menu(paths, window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
        true
    }

    fn deliver_menu(&mut self, paths: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((action, leaf)) = self.menu.pending.take() else {
            return;
        };
        let Some(capture) = self.menu.captured.clone() else {
            return;
        };
        let side = capture.side;
        if self.deliver_actions(&capture, &action, &leaf, paths.clone(), window, cx) {
            return;
        }
        let step = match action.as_str() {
            "open" if !self.menu.context.row_is_file => {
                paths.first().map(|path| Step::Open(path.clone()))
            }
            "toggleHidden" => Some(Step::ToggleHidden),
            "sort" if leaf == flea::ui_logic_mn1::rows::SORT_DEFAULT => Some(Step::ForgetSort),
            "sort" => Some(Step::SortColumn(leaf.clone())),
            _ => None,
        };
        if let Some(step) = step {
            self.retire_menu(window, cx);
            let _ = self.step(side, step, window, cx);
        } else if action == "selectAll" {
            self.retire_menu(window, cx);
            if let Some(host) = &mut self.panes[side] {
                host.selected = (0..host.pane.held.total).collect();
            }
        } else if action == "makeExecutable" {
            self.make_executable(&capture, &paths, window, cx);
        } else if self.clipboard_menu(&capture, &action, &leaf, &paths, window, cx) {
        } else if action == "runScript" {
            let request = format!(
                "{{\"c\":\"actions\",\"op\":\"run\",\"id\":{},\"action\":\"{}\",\"paths\":[{}]}}",
                capture.id,
                escape(&leaf),
                paths
                    .iter()
                    .map(|path| format!("\"{}\"", escape(path)))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            self.menu_send(side, request, cx);
            self.retire_menu(window, cx);
        } else {
            self.menu.captured = None;
            self.menu.activating = false;
            self.menu.reading = false;
            let claimed = std::rc::Rc::new(std::cell::Cell::new(false));
            let chosen = Chosen {
                captured: capture.clone(),
                action,
                leaf,
                paths,
                claimed: claimed.clone(),
            };
            // A family that runs the choice claims it; the deferred check below refuses one nobody claimed.
            self.dispatch(Order::Menu(chosen), window, cx);
            cx.defer_in(window, move |shell, window, cx| {
                if shell
                    .menu
                    .captured
                    .as_ref()
                    .is_some_and(|current| current.side == side)
                {
                    return;
                }
                if !claimed.get() {
                    if !capture.background {
                        shell.menu_send(
                            side,
                            format!(
                                "{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{}}}",
                                capture.id
                            ),
                            cx,
                        );
                    }
                    shell.menu_notice(
                        side,
                        shell.locale.catalog.tr("menu.refusal.unavailable"),
                        cx,
                    );
                }
                if let Some(host) = &shell.panes[side] {
                    let gate = crate::navigation::watch::Gate {
                        menu_open: false,
                        menu_pending: false,
                        ..host.nav.gate
                    };
                    shell.set_gate(side, gate, window, cx);
                }
                cx.notify();
            });
        }
        cx.notify();
    }
}
