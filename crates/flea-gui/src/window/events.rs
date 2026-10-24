use super::{
    lifecycle::Phase,
    locale::{self, Snapshot},
};
use crate::{
    backend::{Event as BackendEvent, Failure},
    state::{labels, Shell},
};
use gpui::{Context, Window};
#[path = "events_filesystem.rs"]
mod filesystem;
#[path = "events_ipc.rs"]
mod ipc;

impl Shell {
    // Platform quit closes each backend's stdin and lets go of it, so its EOF cleanup outlives this process.
    pub(crate) fn stop_for_app_quit(&mut self) {
        self.lifecycle.begin_close();
        self.stop_settings();
        self.presentation.close();
        self._ipc_reader = None;
        self._locale_reader = None;
        self.locale_service = None;
        self._presentation_reader = None;
        self.presentation_service = None;
        self.deadlines = [None, None];
        for host in self.panes.iter_mut().flatten() {
            if let Some(columns) = &mut host.columns {
                columns.timer = None;
            }
            host.transition = None;
            host.pane.swap.mark_shown(false, host.pane.now());
            host.reader = None;
            host.stopped = true;
            if let Some(backend) = host.backend.take() {
                backend.close_stdin();
            }
        }
    }

    pub fn follow_locale(&mut self, service: &locale::Service, cx: &mut Context<Self>) {
        self.locale_service = Some(service.clone());
        let updates = service.subscribe();
        self._locale_reader = Some(cx.spawn(async move |this, cx| {
            while let Ok(snapshot) = updates.recv().await {
                if this
                    .update(cx, |shell, cx| shell.apply_locale(snapshot, cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }

    pub(crate) fn apply_locale(&mut self, locale: Snapshot, cx: &mut Context<Self>) {
        if self.locale.revision == locale.revision {
            return;
        }
        self.rail_locale(&locale.catalog, cx);
        self.labels = labels(&locale);
        for host in self.panes.iter_mut().flatten() {
            host.translated(&locale);
        }
        self.locale = locale;
        self.settings_translated(cx);
        if let Some(chrome) = &self.chrome.view {
            let path = chrome.read_with(cx, |view, _| view.path.clone());
            path.update(cx, |view, _| view.set_catalog(self.locale.catalog.clone()));
        }
        cx.notify();
    }

    pub(crate) fn receive(
        &mut self,
        side: usize,
        event: BackendEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event, BackendEvent::Failed(_) | BackendEvent::Drained)
            && self
                .menu
                .captured
                .as_ref()
                .is_some_and(|capture| capture.side == side)
        {
            self.retire_menu(window, cx);
        }
        if let BackendEvent::Producer(line) = &event {
            if self.search_line(side, line, window, cx) {
                return;
            }
        }
        if matches!(event, BackendEvent::Failed(_) | BackendEvent::Drained) {
            self.search_leave(side, window, cx);
            self.search_landed(side, true, window, cx);
        }
        if let BackendEvent::Producer(line) | BackendEvent::Operations(line) = &event {
            let menu_taken = self.menu_reply(side, line, window, cx);
            if flea::ui_logic_op1::parse(line).is_some() {
                self.operation_line(side, line, window, cx);
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
        if let BackendEvent::TrashLine(line) = &event {
            if self.operation_view(side).is_some() {
                self.operation_line(side, line, window, cx);
            }
            self.trash_line(side, line, cx);
            return;
        }
        if matches!(event, BackendEvent::Failed(_) | BackendEvent::Drained) {
            if let Some(host) = self.trash.as_ref().filter(|host| host.backend_side == side) {
                host.pane.update(cx, |pane, cx| pane.transport_failed(cx));
            }
        }
        match &event {
            BackendEvent::Drained | BackendEvent::Failed(_) => {
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
                        self.path_labels[side] = host.path.clone().into();
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
                return;
            }
            event => event,
        };
        match event {
            BackendEvent::Plugins(_) => unreachable!("plugins handled above"),
            BackendEvent::FilesystemDetails { .. } => {
                unreachable!("filesystem fanout handled above")
            }
            BackendEvent::Peeked(_) => unreachable!(),
            BackendEvent::TrashLine(_) => unreachable!(),
            BackendEvent::Path(_) => unreachable!(),
            BackendEvent::Producer(_) | BackendEvent::Operations(_) => {
                unreachable!("producer handled above")
            }
            BackendEvent::Session { .. } => unreachable!(),
            BackendEvent::ListDetails(reply) => {
                self.list_details(side, reply, cx);
                return;
            }
            BackendEvent::Filesystem(facts) => {
                if facts.path == host.path {
                    let fs = &mut self.surfaces[side].filesystem;
                    fs.enter(&facts.path);
                    fs.answered = true;
                    fs.facts = Some(facts);
                    cx.notify();
                }
                return;
            }
            BackendEvent::Drained => {
                host.stopped = true;
                host.backend = None;
                self.retire_status_owner(side, window, cx);
                self.lifecycle.drained(side);
                if self.lifecycle.phase() == Phase::Closed {
                    window.remove_window();
                }
            }
            BackendEvent::Failed(error) => {
                if host.pane.fail("backend", "", "", 0, host.pane.now()) {
                    self.surfaces[side].refuse(crate::window::surfaces::Refusal {
                        operation: "backend".into(),
                        message: String::new(),
                        mode: 0,
                    });
                }
                host.changed(&self.locale);
                host.record_error(error, &self.locale);
                self.retire_status_owner(side, window, cx);
                self.status_refused("backend", "", "", cx);
            }
            BackendEvent::Refused {
                operation,
                path,
                message,
                mode,
            } => {
                let current =
                    path.is_empty() || path == host.pane.path() || path == host.pane.place();
                let named = if path == host.path {
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
            BackendEvent::Changed { path } => {
                let now = host.pane.now();
                host.nav.changed(&host.pane, &path, now);
                self.arm_watch(side, window, cx);
                return;
            }
            BackendEvent::Located(located) => {
                let taken = match located.id {
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
                if taken {
                    host.input.selection.sync(&host.selected);
                    let now = host.pane.now();
                    host.nav.settle(&host.pane, now);
                    self.run_navigation(side, window, cx);
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
                        }
                        host.changed(&self.locale);
                        self.surfaces[side].filesystem.enter(&host.path);
                        self.path_labels[side] = host.path.clone().into();
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
        self.search_landed(side, false, window, cx);
        self.settle_dual(window, cx);
        self.after_navigation(side, &before, window, cx);
        self.arm_transition(side, window, cx);
        if side == 0 {
            self.ensure_ipc_reader(window, cx);
        }
        cx.notify();
    }
}
