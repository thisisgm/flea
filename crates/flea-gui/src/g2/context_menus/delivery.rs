//! Snapshot retirement, backend validation and captured choices delivered to their producer owners.
use super::{join::Chosen, menu::Outcome};
use crate::{
    state::Shell,
    window::{dispatch::Order, navigate::Step},
};
use flea::{
    json::escape,
    jsondoc::Json,
    ui_logic_context_menus::{build::App, refresh},
};
use gpui::{Context, Window};

pub(crate) fn string<'a>(value: &'a Json, key: &str) -> &'a str {
    value.get(key).and_then(Json::as_str).unwrap_or("")
}

impl Shell {
    pub(crate) fn retire_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.strip.menu = None;
        self.menu.text = None;
        self.cancel_plugin_input(cx);
        self.menu.tags.retire();
        let place = self.menu.place.take().is_some();
        let rail = self.menu.rail.take().is_some();
        if place || rail {
            self.rail_menu_hold(false, cx);
        }
        self.providers_menu_closed();
        self.menu.menu.borrow_mut().close();
        self.menu.presentation.cancel();
        self.menu.flyout = None;
        self.menu.header = false;
        self.menu.tags_reading = false;
        self.menu.pending = None;
        self.menu.from_sheet = false;
        self.menu.external = None;
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
            self.free_menu_gate(capture.side, window, cx);
        }
        self.keyboard.context = flea::keys::Context::Listing;
        self.keyboard.disarm();
        // Sidebar.qml openRailMenu: every rail row's menu closes back to the rail it opened from.
        if place || rail {
            window.focus(&self.dual_focus.rail, cx);
            self.keyboard.context = flea::keys::Context::Rail;
        }
        cx.notify();
    }

    pub(crate) fn activate_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.activate_place_menu(window, cx) {
            return;
        }
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
        if !capture.background && capture.ready && self.provider_before_activation(window, cx) {
            return;
        }
        if refresh::validate_choice(
            &self.menu_choice_entries(&action, &leaf),
            &action,
            &leaf,
            false,
        )
        .is_err()
        {
            self.menu_outcome(Outcome::Refused("menu.refusal.unavailable"), window, cx);
            return;
        }
        if capture.background {
            // LockedMenu.js: every locked row acts on the denied folder itself, which may be a child of the pane's path.
            let paths = self
                .locked_folder(capture.side)
                .map(|(folder, _)| vec![folder])
                .unwrap_or_default();
            self.deliver_menu(paths, window, cx);
        } else if capture.ready {
            self.menu.activating = true;
            if let Some(host) = &mut self.panes[capture.side] {
                host.nav.gate.menu_pending = true;
            }
            // Dropbox rows name the account root the backend already took as this menu's destination.
            let dest = if matches!(action.as_str(), "dropbox" | "sharelink") {
                format!(
                    ",\"dest\":\"{}\"",
                    escape(&self.provider_destination(capture.side))
                )
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
        // The rclone sender, then the archive and flash host, answer their own lines first, so none is forwarded as an unclaimed reply.
        if self.rclone_line(side, line, window, cx) {
            return true;
        }
        if self.archive_line(side, line, window, cx) {
            return true;
        }
        let Ok(reply) = flea::jsondoc::parse(line) else {
            return false;
        };
        if self.actions_reply(side, line, cx) || self.menu_tag_line(side, &reply, window, cx) {
            return true;
        }
        if self.menu_discovery_reply(side, line, &reply, window, cx) {
            return true;
        }
        if string(&reply, "t") != "menuaction" {
            return false;
        }
        let id = flea::json::field_usize(line, "id").unwrap_or(0) as u64;
        if string(&reply, "op") == "openWith" && self.menu_launched(side, id, &reply, cx) {
            return true;
        }
        let Some(capture) = self
            .menu
            .captured
            .clone()
            .filter(|c| c.id == id && c.side == side)
        else {
            return false;
        };
        let operation = string(&reply, "op");
        if !matches!(
            operation,
            "snapshot" | "validate" | "applications" | "tagstates"
        ) {
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
                self.menu.context.open_with_late = false;
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
                // Only a capture's first snapshot reply may start discovery; a later one would ask again and hold a chosen row.
                let first = !std::mem::replace(&mut capture.ready, true);
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
                self.menu_target_modes(&reply, window, cx);
                self.menu.context.tag_submenu = None;
                if self.menu.tags.asking.is_none() {
                    self.ask_tag_states(true, cx);
                } else if self.menu.flyout == Some("tags") {
                    self.menu.tags_reading = true;
                }
                if self.menu.pending.is_some() {
                    self.activate_menu(window, cx);
                } else if first {
                    self.discover_menu(window, cx);
                }
            }
            "applications" => {
                self.menu.reading = false;
                self.menu.context.open_with_loaded = true;
                let late = std::mem::replace(&mut self.menu.context.open_with_late, false);
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
                // refresh_menu leaves a card it already opened alone, so the card the deadline opened takes the answer here.
                if late {
                    let fresh = self
                        .menu_entries()
                        .into_iter()
                        .find(|e| e.action == "openWith");
                    if let Some(entry) = fresh {
                        self.menu
                            .menu
                            .borrow_mut()
                            .replace_leaves("openWith", entry.submenu);
                        cx.notify();
                    }
                }
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
                let live = self.menu_choice_entries(action, leaf);
                if refresh::validate_choice(&live, action, leaf, false).is_err() {
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

    // The pane's keys and watch go back to the listing once its menu has no snapshot left to answer.
    pub(super) fn free_menu_gate(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = &self.panes[side] {
            let gate = crate::navigation::watch::Gate {
                menu_open: false,
                menu_pending: false,
                ..host.nav.gate
            };
            self.set_gate(side, gate, window, cx);
        }
    }

    pub(crate) fn deliver_menu(
        &mut self,
        paths: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((action, leaf)) = self.menu.pending.take() else {
            return;
        };
        let Some(capture) = self.menu.captured.clone() else {
            return;
        };
        if let Some(waiting) = self.menu.external.take() {
            waiting.answer(crate::window::external_action::ran());
        }
        let side = capture.side;
        if action == "sendRclone" {
            self.rclone_choose(&capture, &leaf, paths, window, cx);
            return;
        }
        if self.header_menu_choose(&action, window, cx) {
            return;
        }
        if self.deliver_actions(&capture, &action, &leaf, paths.clone(), window, cx) {
            return;
        }
        if self.neighbour_selection(&capture, &action, window, cx) {
            return;
        }
        if action == "sort" && self.neighbour_sort(&capture, &leaf, window, cx) {
            return;
        }
        let step = match action.as_str() {
            "open" if !self.menu.context.row_is_file && capture.rows.len() <= 1 => {
                paths.first().map(|path| Step::Open(path.clone()))
            }
            "sort" if leaf == flea::ui_logic_context_menus::rows::SORT_DEFAULT => {
                Some(Step::ForgetSort)
            }
            "sort" => Some(Step::SortColumn(leaf.clone())),
            _ => None,
        };
        if let Some(step) = step {
            self.retire_menu(window, cx);
            let _ = self.step(side, step, window, cx);
        } else if action == "openWith" && capture.target.is_some() {
            self.neighbour_open_with(&capture, &leaf, window, cx);
        } else if action == "makeExecutable" {
            self.make_executable(&capture, &paths, window, cx);
        } else if action == "addToShelf" {
            self.menu_shelve(&capture, &paths, window, cx);
        } else if action == "openWith" && !capture.background {
            self.menu_open_with(&capture, &leaf, &paths, window, cx);
        } else if action == "tags" && !capture.background {
            self.menu_tag(&capture, &leaf, window, cx);
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
            self.menu.actions.run_named_on(
                side,
                usize::try_from(capture.id).unwrap_or(usize::MAX),
                &leaf,
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
                    shell.menu_unclaimed(&capture, cx);
                }
                shell.free_menu_gate(side, window, cx);
                cx.notify();
            });
        }
        cx.notify();
    }
}
