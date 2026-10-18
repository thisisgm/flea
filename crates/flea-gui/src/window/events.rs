use super::{
    lifecycle::Phase,
    locale::{self, Snapshot},
};
use crate::{
    backend::{Event as BackendEvent, Failure},
    ipc::{server::Event, Surface},
    state::{labels, Shell},
};
use flea::protocol::Reply;
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn ensure_ipc_reader(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(events) = self.ipc_events.take() else {
            return;
        };
        self._ipc_reader = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if cx
                    .update(|window, cx| this.update(cx, |shell, cx| shell.ipc(event, window, cx)))
                    .is_err()
                {
                    break;
                }
            }
        }));
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
        self.labels = labels(&locale);
        for host in self.panes.iter_mut().flatten() {
            host.translated(&locale);
        }
        self.locale = locale;
        cx.notify();
    }

    pub(crate) fn ipc(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::Call { request, answer } => {
                let Some(host) = &self.panes[self.focus_side] else {
                    return;
                };
                let surface = Surface {
                    path: &host.path,
                    total: host.pane.held.total,
                    cursor: host.pane.cursor,
                    selected: &host.selected,
                };
                let _ = answer.try_send(surface.dispatch(request));
            }
            Event::Quit => {
                self.close(window, cx);
            }
        }
    }

    pub(crate) fn receive(
        &mut self,
        side: usize,
        event: BackendEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        match event {
            BackendEvent::Drained => {
                host.stopped = true;
                host.backend = None;
                self.lifecycle.drained(side);
                if self.lifecycle.phase() == Phase::Closed {
                    window.remove_window();
                }
            }
            BackendEvent::Failed(error) => host.record_error(error, &self.locale),
            BackendEvent::Reply(Reply::Error { path, message, .. }) if path == host.path => {
                host.record_error(
                    Failure {
                        key: "window.backend_report",
                        detail: message,
                    },
                    &self.locale,
                );
            }
            BackendEvent::Reply(reply) => match host.pane.receive(reply) {
                Ok(true) => {
                    if !host.answered {
                        host.apply_select();
                    }
                    host.answered = true;
                    host.error = None;
                    host.failure = None;
                    host.status = self
                        .locale
                        .catalog
                        .trn("items.count", host.pane.held.total as i64)
                        .into();
                    self.deadlines[side] = None;
                    self.seam.rows(&host.pane);
                }
                Ok(false) => return,
                Err(message) => host.record_error(
                    Failure {
                        key: "window.backend_protocol",
                        detail: message,
                    },
                    &self.locale,
                ),
            },
        }
        if side == 0 {
            self.ensure_ipc_reader(window, cx);
        }
        cx.notify();
    }
}
