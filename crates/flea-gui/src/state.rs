use crate::{
    backend::{Backend, Failure},
    ipc::server::Event,
    list::seam::Seam,
    window::{
        composition::{Geometry, Labels, Slots},
        lifecycle::{Lifecycle, Phase},
        locale::{self, Snapshot},
        pane::PaneHost,
        startup::{Launch, View},
    },
};
use gpui::{Context, FocusHandle, SharedString, Task, Window};

pub struct Shell {
    pub launch: Launch,
    pub slots: Slots,
    pub lifecycle: Lifecycle,
    pub view: View,
    pub focus_side: usize,
    pub(crate) panes: [Option<PaneHost>; 2],
    pub(crate) locale: Snapshot,
    pub(crate) labels: Labels,
    pub(crate) path_labels: [SharedString; 2],
    pub(crate) focus: FocusHandle,
    pub(crate) seam: Seam,
    pub(crate) ipc_events: Option<async_channel::Receiver<Event>>,
    pub(crate) _ipc_reader: Option<Task<()>>,
    pub(crate) _locale_reader: Option<Task<()>>,
    pub(crate) locale_service: Option<locale::Service>,
    _app_quit: Option<gpui::Subscription>,
    pub(crate) deadlines: [Option<Task<()>>; 2],
}

pub(crate) fn labels(locale: &Snapshot) -> Labels {
    Labels {
        name: locale.catalog.tr("window.column_name").into(),
        size: locale.catalog.tr("window.column_size").into(),
        modified: locale.catalog.tr("window.column_modified").into(),
    }
}

impl Shell {
    pub fn new(
        launch: Launch,
        locale: Snapshot,
        events: async_channel::Receiver<Event>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let seam = Seam::new();
        seam.window_opened();
        let view = launch.settings.view;
        let focus_side = launch.focus;
        let path_labels = launch.paths.clone().map(SharedString::from);
        let mut shell = Self {
            launch,
            view,
            focus_side,
            path_labels,
            labels: labels(&locale),
            locale,
            focus,
            seam,
            panes: [None, None],
            slots: Slots::default(),
            lifecycle: Lifecycle::default(),
            ipc_events: Some(events),
            _ipc_reader: None,
            _locale_reader: None,
            locale_service: None,
            _app_quit: None,
            deadlines: [None, None],
        };
        shell._app_quit = Some(cx.on_app_quit(|shell, _| {
            let pending: Vec<_> = shell
                .panes
                .iter()
                .flatten()
                .filter_map(|host| host.backend.as_ref())
                .map(|backend| {
                    backend.quit();
                    backend.drained()
                })
                .collect();
            async move {
                for backend in pending {
                    let _ = backend.recv().await;
                }
            }
        }));
        shell.start_pane(0, window, cx);
        if view == View::Dual {
            shell.start_pane(1, window, cx);
        }
        if shell.panes[0].as_ref().is_some_and(|host| host.stopped) {
            shell.ensure_ipc_reader(window, cx);
        }
        shell
    }

    pub(crate) fn has_tabs(&self) -> bool {
        self.slots.tabs.is_some()
            || self
                .launch
                .settings
                .data
                .get("lastTabs")
                .and_then(|tabs| tabs.get("paths"))
                .and_then(flea::jsondoc::Json::as_array)
                .is_some_and(|paths| paths.len() > 1)
    }

    pub(crate) fn geometry(&self, window: &Window) -> Geometry {
        let viewport = self.seam.viewport().unwrap_or(window.viewport_size());
        Geometry::measure(
            f32::from(viewport.width),
            f32::from(viewport.height),
            &self.launch.settings,
            self.view,
            self.has_tabs(),
        )
    }

    fn start_pane(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes[side].is_some() || !self.lifecycle.attach(side) {
            return;
        }
        let height = self.seam.viewport().map_or_else(
            || {
                self.geometry(window)
                    .rows_height(self.view == View::Dual, self.slots.strip_height)
            },
            |viewport| f32::from(viewport.height),
        );
        let mut host = PaneHost::new(
            self.launch.paths[side].clone(),
            self.launch.select[side].clone(),
            height,
            &self.locale,
        );
        match Backend::start(
            host.path.clone(),
            host.select.clone(),
            host.pane.first_count(),
        ) {
            Ok(backend) => {
                let replies = backend.replies.clone();
                host.reader = Some(cx.spawn_in(window, async move |this, cx| {
                    while let Ok(event) = replies.recv().await {
                        if cx
                            .update(|window, cx| {
                                this.update(cx, |shell, cx| shell.receive(side, event, window, cx))
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }));
                host.backend = Some(backend);
            }
            Err(error) => {
                host.stopped = true;
                host.record_error(
                    Failure {
                        key: "window.backend_reader",
                        detail: error.to_string(),
                    },
                    &self.locale,
                );
                self.lifecycle.drained(side);
            }
        }
        self.panes[side] = Some(host);
        self.arm_transition(side, window, cx);
        self.arm_listing_wait(side, window, cx);
    }

    pub fn set_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if self.lifecycle.phase() != Phase::Open {
            return;
        }
        self.view = view;
        if view == View::Dual {
            self.start_pane(1, window, cx);
        } else {
            self.focus_side = 0;
        }
        cx.notify();
    }

    pub fn focus_pane(&mut self, side: usize, cx: &mut Context<Self>) {
        if side < 2 && (side == 0 || self.view == View::Dual) && self.panes[side].is_some() {
            self.focus_side = side;
            cx.notify();
        }
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.lifecycle.begin_close() {
            for host in self.panes.iter().flatten() {
                if let Some(backend) = &host.backend {
                    backend.quit();
                }
            }
            cx.notify();
        }
        let closed = self.lifecycle.phase() == Phase::Closed;
        if closed {
            window.remove_window();
        }
        closed
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
pub(crate) mod tests;
