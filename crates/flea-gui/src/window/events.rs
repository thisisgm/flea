use super::lifecycle::Phase;
use crate::{
    backend::{Event as BackendEvent, Failure},
    state::Shell,
};
use gpui::{Context, Window};
#[path = "events_backend.rs"]
mod backend;
#[path = "events_filesystem.rs"]
mod filesystem;
#[path = "events_ipc.rs"]
mod ipc;
#[path = "events_quit.rs"]
mod quit;
#[path = "events_routes.rs"]
mod routes;
#[path = "events_slow.rs"]
mod slow;
// Only the status tests read the sentence directly; the window reads it through slow_line.
#[cfg(test)]
pub(crate) use slow::slow_sentence;

impl Shell {
    pub(crate) fn receive(
        &mut self,
        side: usize,
        event: BackendEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(event) = self.backend_line(side, event, cx) else {
            return;
        };
        let Some(event) = self.plugin_place_event(side, event) else {
            return;
        };
        let event = self.filesystem_flags(side, event);
        // PaneWire.qml onFailed: a dead backend with a rename out names the rename, in the state slot and the bar.
        let rename_out = matches!(event, BackendEvent::Failed(_))
            && self
                .rename
                .edit
                .as_ref()
                .is_some_and(|edit| edit.side == side && edit.pending.is_some());
        if self.git_event(side, &event, cx) || self.tags_event(side, &event, window, cx) {
            return;
        }
        let renamed = self.rename_event(side, &event, window, cx);
        self.rename_sync_gate(window, cx);
        if renamed {
            return;
        }
        if let BackendEvent::Peeked(document) = &event {
            self.tab_peeked(side, document, window, cx);
        }
        if self.permissions_event(side, &event, window, cx) {
            return;
        }
        if matches!(event, BackendEvent::Failed(_) | BackendEvent::Drained)
            && self
                .menu
                .captured
                .as_ref()
                .is_some_and(|capture| capture.side == side)
        {
            self.retire_menu(window, cx);
        }
        if self.route_surface_event(side, &event, window, cx) {
            return;
        }
        if let BackendEvent::Producer(line) = &event {
            if self.filter_line(side, line, cx) {
                self.filter_answered(side, window, cx);
                return;
            }
            if self.search_line(side, line, window, cx) {
                return;
            }
        }
        if self.output_event(side, &event, cx) {
            return;
        }
        if matches!(event, BackendEvent::Failed(_) | BackendEvent::Drained) {
            self.search_leave(side, window, cx);
            self.search_landed(side, true, window, cx);
        }
        if let BackendEvent::Producer(line) | BackendEvent::Operations(line) = &event {
            self.slow_line(line, cx);
            if self.open_with_line(side, line, cx) {
                return;
            }
            if self.fileops_line(side, line, window, cx) {
                return;
            }
            let menu_taken = self.menu_reply(side, line, window, cx);
            if let Some(parsed) = flea::ui_logic_op1::parse(line) {
                self.operation_line(side, line, window, cx);
                if let flea::ui_logic_op1::Event::TransferDone(done) = parsed {
                    self.transfer_refresh(side, window, cx);
                    self.retry_failed(side, &done.retry_paths, window, cx);
                }
            }
            if !menu_taken {
                cx.emit(crate::g2::mn1::join::ProducerReply {
                    side,
                    line: line.clone(),
                });
            }
            return;
        }
        let event = match event {
            BackendEvent::Path(answer) => {
                // Only jumped replies use this variant; peeks keep the complete document below.
                let _stale_jump = self.path_answer(side, answer, window, cx);
                return;
            }
            BackendEvent::Peeked(document) => {
                if let Some(answer) = crate::path::answer(&document) {
                    if self.path_answer(side, answer, window, cx).is_none() {
                        return;
                    }
                }
                // The protected unclaimed return retains the original complete Columns wire input.
                BackendEvent::Peeked(document)
            }
            event => event,
        };
        if let BackendEvent::Plugins(line) = &event {
            self.settings_plugins_reply(line, cx);
            return;
        }
        if matches!(event, BackendEvent::Drained | BackendEvent::Failed(_)) {
            self.settings_plugins_failed(side, cx);
        }
        if self.columns_event(side, &event, window, cx) {
            return;
        }
        if self.trash_event(side, &event, window, cx) {
            return;
        }
        if matches!(event, BackendEvent::Failed(_) | BackendEvent::Drained) {
            if let Some(host) = self.trash.as_ref().filter(|host| host.backend_side == side) {
                host.pane.update(cx, |pane, cx| pane.transport_failed(cx));
            }
        }
        match &event {
            BackendEvent::Drained | BackendEvent::Failed(_) => {
                self.fileops_reset(side);
                self.operation_failed(side, "backend", "", "", cx)
            }
            BackendEvent::Refused {
                operation,
                path,
                message,
                ..
            } => self.operation_failed(side, operation, path, message, cx),
            _ => {}
        }
        let event = self.filesystem_details(side, event, cx);
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        if host.grid_refusal(&event) {
            cx.notify();
        }
        let before = host.pane.path().to_owned();
        let event = match event {
            BackendEvent::Plugins(_) => unreachable!("plugins handled above"),
            BackendEvent::Session { generation, reply } => {
                if host.pane.swap.session.drops(generation) {
                    return;
                }
                match host
                    .pane
                    .receive_session(reply, generation, host.pane.now())
                {
                    Ok(true) => {
                        host.search.rows_generation = generation;
                        host.changed(&self.locale);
                        self.surfaces[side].filesystem.enter(&host.path);
                        let name = self.locale.catalog.tr("places.row.recent");
                        self.path_labels[side] = super::recent::path_label(host, &name);
                        self.seam.rows(&host.pane);
                        self.settle_dual(window, cx);
                        cx.notify();
                    }
                    Ok(false) => {}
                    Err(key) => host.record_error(
                        Failure {
                            key: "window.backend_protocol",
                            detail: self.locale.catalog.tr(&key),
                        },
                        &self.locale,
                    ),
                }
                self.opener_archive_ready(side, window, cx);
                self.ipc_listing_answered(side, window, cx);
                self.search_rows_landed(side, window, cx);
                return;
            }
            event => event,
        };
        match event {
            BackendEvent::Plugins(_) => unreachable!("plugins handled above"),
            BackendEvent::FilesystemDetails { .. } | BackendEvent::FilesystemSystem { .. } => {
                unreachable!("filesystem fanout handled above")
            }
            BackendEvent::Peeked(_) => unreachable!(),
            BackendEvent::TrashLine(_) => unreachable!(),
            BackendEvent::Path(_) => unreachable!(),
            BackendEvent::Producer(_) | BackendEvent::Operations(_) => {
                unreachable!("producer handled above")
            }
            BackendEvent::Session { .. } => unreachable!(),
            BackendEvent::Garbled(_) => unreachable!("refused as parse above"),
            BackendEvent::ListDetails(reply) => {
                self.list_details(side, reply, cx);
                return;
            }
            BackendEvent::Filesystem(facts) => {
                let supported = self.can_tag(side);
                self.filesystem_answer(side, facts, cx);
                if supported != self.can_tag(side) {
                    self.refresh_tag_capability(side, window, cx);
                }
                return;
            }
            BackendEvent::Drained => {
                host.stopped = true;
                host.backend = None;
                self.chrome.jumps[side] = 0;
                self.retire_status_owner(side, window, cx);
                self.lifecycle.drained(side);
                if self.lifecycle.phase() == Phase::Closed {
                    window.remove_window();
                }
            }
            BackendEvent::Failed(error) => {
                self.backend_failed(side, error, rename_out, window, cx);
            }
            BackendEvent::Refused {
                operation,
                path,
                message,
                mode,
            } => {
                let current =
                    path.is_empty() || path == host.pane.path() || path == host.pane.place();
                // A denied row of the shown listing is this folder, so the footer says the pane's own sentence too.
                let listed = crate::navigation::history::parent_of(&path) == host.path;
                let this_folder = path == host.path
                    || (operation == "scan"
                        && listed
                        && flea::ui_logic_fb1::errors::denied(&operation, &message));
                let named = if this_folder {
                    ""
                } else {
                    path.rsplit('/').next().unwrap_or_default()
                };
                let ended = host
                    .pane
                    .fail(&operation, &path, &message, mode, host.pane.now());
                if ended {
                    self.surfaces[side].refuse(crate::window::surfaces::Refusal {
                        operation: operation.clone(),
                        message: message.clone(),
                        mode,
                    });
                    host.changed(&self.locale);
                }
                if ended || (operation == "fsinfo" && current) {
                    self.surfaces[side].filesystem.settle_without_facts();
                }
                host.nav.after_failure(&mut host.pane, &operation);
                self.run_navigation(side, window, cx);
                if !ended {
                    if !matches!(operation.as_str(), "scan" | "list" | "window") || current {
                        self.status_refused(&operation, &message, named, cx);
                    }
                    return;
                }
                self.status_refused(&operation, &message, named, cx);
            }
            BackendEvent::ReloadChanged(changed) => {
                host.nav.reload_changed = Some(changed);
                return;
            }
            BackendEvent::Changed { path } => {
                let now = host.pane.now();
                host.nav.changed(&host.pane, &path, now);
                self.arm_watch(side, window, cx);
                return;
            }
            BackendEvent::Located(located) => {
                if self.retry_located(side, &located, window, cx) {
                    return;
                }
                let tab_taken = self.tab_located(side, &located);
                let Some(host) = self.panes[side].as_mut() else {
                    return;
                };
                let taken = match located.id {
                    Some(id)
                        if host.nav.take_selected(
                            &mut host.pane,
                            &located.directory,
                            id,
                            located.ok,
                            &located.matches,
                            &mut host.selected,
                        ) =>
                    {
                        true
                    }
                    Some(id) => host.nav.take_located_many(
                        &host.pane,
                        &located.directory,
                        id,
                        located.ok,
                        &located.matches,
                        &mut host.selected,
                    ),
                    None => host.nav.take_located(
                        &mut host.pane,
                        &located.directory,
                        &located.path,
                        located.index,
                    ),
                };
                if taken || tab_taken {
                    host.input.selection.sync(&host.selected);
                    host.take_lone_mark();
                    let now = host.pane.now();
                    host.nav.settle(&host.pane, now);
                    self.tab_landed(side, window, cx);
                    self.run_navigation(side, window, cx);
                    self.replay_queued(side, window, cx);
                    self.arm_watch(side, window, cx);
                    cx.notify();
                }
                return;
            }
            BackendEvent::Paths(paths) => {
                if host.nav.take_paths(&mut host.pane, &paths) {
                    let now = host.pane.now();
                    host.nav.settle(&host.pane, now);
                    self.run_navigation(side, window, cx);
                    self.arm_watch(side, window, cx);
                }
                return;
            }
            BackendEvent::Reply(reply) => {
                let mut said = None;
                let failure = match &reply {
                    flea::protocol::Reply::Error {
                        operation,
                        message,
                        mode,
                        ..
                    } => Some((operation.clone(), message.clone(), *mode)),
                    _ => None,
                };
                match host.pane.receive(reply) {
                    Ok(true) => {
                        if let Some((operation, message, mode)) = failure {
                            said = Some((operation.clone(), message.clone()));
                            self.surfaces[side].refuse(crate::window::surfaces::Refusal {
                                operation,
                                message,
                                mode,
                            });
                            self.surfaces[side].filesystem.settle_without_facts();
                        }
                        host.changed(&self.locale);
                        self.surfaces[side].filesystem.enter(&host.path);
                        let name = self.locale.catalog.tr("places.row.recent");
                        self.path_labels[side] = super::recent::path_label(host, &name);
                        if host.answered {
                            self.deadlines[side] = None;
                        }
                        self.seam.rows(&host.pane);
                    }
                    Ok(false) => {
                        self.after_navigation(side, &before, window, cx);
                        self.arm_transition(side, window, cx);
                        return;
                    }
                    Err(message) => {
                        said = Some(("protocol".into(), String::new()));
                        if host.pane.fail("protocol", "", "", 0, host.pane.now()) {
                            self.surfaces[side].refuse(crate::window::surfaces::Refusal {
                                operation: "protocol".into(),
                                message: String::new(),
                                mode: 0,
                            });
                        }
                        host.changed(&self.locale);
                        host.record_error(
                            Failure {
                                key: "window.backend_protocol",
                                detail: self.locale.catalog.tr(&message),
                            },
                            &self.locale,
                        );
                    }
                }
                if let Some((operation, message)) = said {
                    self.status_refused(&operation, &message, "", cx);
                }
            }
        }
        self.opener_archive_ready(side, window, cx);
        self.search_landed(side, false, window, cx);
        self.settle_dual(window, cx);
        self.after_navigation(side, &before, window, cx);
        self.arm_transition(side, window, cx);
        self.fileops_arrival(side, window, cx);
        self.ipc_listing_answered(side, window, cx);
        cx.notify();
    }
}
