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

const FALLBACK_HEADER_HEIGHT: f32 = 27.;

pub struct Shell {
    pub(crate) capture: crate::capture::Capture,
    pub(crate) visual: crate::visual::Visual,
    pub(crate) presentation: crate::window::frame_feedback::Presentation,
    pub launch: Launch,
    pub slots: Slots,
    pub(crate) chrome: crate::path::join::Chrome,
    pub lifecycle: Lifecycle,
    pub view: View,
    pub focus_side: usize,
    pub keyboard: crate::window::input::Keyboard,
    pub input: crate::input::focus::Focus,
    pub(crate) panes: [Option<PaneHost>; 2],
    pub(crate) surfaces: [crate::window::surfaces::Surfaces; 2],
    pub(crate) settings: crate::window::settings::Host,
    pub(crate) menu: crate::g2::mn1::join::Joined,
    pub(crate) operations: crate::g2::op1::join::Sessions,
    pub(crate) clipboard: crate::g2::dg1::join::Joined,
    pub(crate) overlay: Option<gpui::Entity<crate::g2::op1::host::Overlay>>,
    pub(crate) output: crate::window::output::Join,
    pub(crate) trash: Option<crate::g2::pl3::join::TrashHost>,
    pub(crate) rail: Option<crate::window::rail::Join>,
    /// Live compositor rounding, square until it is readable; visual scenes can override it.
    pub(crate) corner: f32,
    pub(crate) locale: Snapshot,
    pub(crate) labels: Labels,
    pub(crate) path_labels: [SharedString; 2],
    pub(crate) focus: FocusHandle,
    pub(crate) seam: Seam,
    pub(crate) ipc_events: Option<async_channel::Receiver<Event>>,
    pub(crate) _ipc_reader: Option<Task<()>>,
    /// True until the first listing answers; calls meanwhile are refused at once, never queued.
    pub(crate) ipc_cold: bool,
    pub(crate) _locale_reader: Option<Task<()>>,
    pub(crate) locale_service: Option<locale::Service>,
    pub(crate) presentation_service: Option<crate::presentation::Service>,
    pub(crate) _presentation_reader: Option<Task<()>>,
    pub(crate) font_key: Option<(
        crate::presentation::Fonts,
        flea::ui_logic_st1::textsize::Stored,
    )>,
    _app_quit: Option<gpui::Subscription>,
    pub(crate) dual_focus: crate::dual::focus::Focus,
    pub(crate) dual_pending: bool,
    pub(crate) dual_written: Option<String>,
    pub(crate) dual_drag: Option<crate::dual::Drag>,
    pub(crate) dual_hover: bool,
    pub(crate) dual_blur: Option<gpui::Subscription>,
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
        let capture = crate::capture::Capture::new(window, cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let seam = Seam::new();
        seam.window_opened();
        let view = launch.settings.view;
        let focus_side = launch.focus;
        let path_labels = launch.paths.clone().map(SharedString::from);
        let mut dual_focus = crate::dual::focus::Focus::new(cx);
        dual_focus.last_side = usize::from(
            launch
                .settings
                .data
                .get("dual")
                .and_then(|v| v.get("focus"))
                .and_then(flea::jsondoc::Json::as_f64)
                == Some(1.),
        );
        let mut shell = Self {
            capture,
            visual: crate::visual::Visual::default(),
            presentation: Default::default(),
            launch,
            view,
            focus_side,
            keyboard: crate::window::input::Keyboard::default(),
            input: Default::default(),
            path_labels,
            labels: labels(&locale),
            locale,
            focus,
            seam,
            panes: [None, None],
            surfaces: Default::default(),
            menu: Default::default(),
            overlay: None,
            output: Default::default(),
            operations: Default::default(),
            clipboard: Default::default(),
            settings: Default::default(),
            trash: None,
            rail: None,
            corner: crate::g2::st1::seams::COMPOSITOR_CORNER_DEFAULT,
            slots: Slots::default(),
            chrome: Default::default(),
            lifecycle: Lifecycle::default(),
            ipc_events: Some(events),
            _ipc_reader: None,
            ipc_cold: true,
            _locale_reader: None,
            locale_service: None,
            presentation_service: None,
            _presentation_reader: None,
            font_key: None,
            _app_quit: None,
            deadlines: [None, None],
            dual_focus,
            dual_pending: false,
            dual_written: None,
            dual_drag: None,
            dual_hover: false,
            dual_blur: None,
        };
        // Returns at once: no kill and no wait inside the platform's shutdown budget.
        shell._app_quit = Some(cx.on_app_quit(|shell, _| {
            shell.stop_for_app_quit();
            std::future::ready(())
        }));
        shell.subscribe_blur(window, cx);
        shell.start_pane(0, window, cx);
        if view == View::Dual {
            shell.start_pane(1, window, cx);
        }
        shell.mount_rail(cx);
        shell.mount_trash(window, cx);
        if let Some(host) = &shell.trash {
            #[cfg(test)]
            constructor_fixture::configure_monitor(&host.pane, cx);
            host.pane.update(cx, |pane, cx| pane.watch(cx));
        }
        shell.start_ipc_reader(window, cx);
        if shell.panes[0].as_ref().is_some_and(|host| host.stopped) {
            shell.ensure_ipc_reader(window, cx);
        }
        shell.follow_frame_feedback(window, cx);
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
        self.geometry_for(self.view, window)
    }

    pub(crate) fn geometry_for(&self, view: View, window: &Window) -> Geometry {
        let viewport = self.seam.viewport().unwrap_or(window.viewport_size());
        let mut geometry = Geometry::with_header_min(
            f32::from(viewport.width),
            f32::from(viewport.height),
            &self.launch.settings,
            view,
            self.has_tabs(),
            self.font_key.as_ref().map_or(FALLBACK_HEADER_HEIGHT, |_| {
                crate::list::Metrics::for_typeface(&self.active_typeface()).header
            }),
            self.dual_min(),
        );
        let detail = self.surfaces[0].detail_height;
        geometry.status.y = (geometry.status.y - detail).max(0.);
        geometry.status.height += detail;
        geometry.rail.height = (geometry.rail.height - detail).max(0.);
        for pane in &mut geometry.panes {
            pane.height = (pane.height - detail).max(0.);
        }
        if view != View::Dual && self.slots.covers[0].is_some() {
            geometry.cover_single();
        }
        geometry
    }

    pub(crate) fn start_pane(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes[side].is_some() || !self.lifecycle.attach(side) {
            return;
        }
        let height = self.seam.viewport().map_or_else(
            || {
                self.geometry(window)
                    .content_height(self.view, self.slots.strip_height)
            },
            |viewport| f32::from(viewport.height),
        );
        let typeface = cx
            .try_global::<crate::presentation::Fonts>()
            .map(|fonts| fonts.typeface(&self.launch.settings.data))
            .unwrap_or_else(crate::list::Typeface::omarchy_default);
        let mut host = PaneHost::with_typeface(
            self.launch.paths[side].clone(),
            self.launch.select[side].clone(),
            height,
            &self.locale,
            typeface,
        );
        self.configure_settings_pane(&mut host, cx);
        host.list.borrow_mut().view.stored =
            crate::list::columns::StoredWidths::from_settings(&self.launch.settings.data);
        host.pane.use_executor(cx.background_executor().clone());
        // One folder memory per window: both panes read and write the same maps.
        let (folders, writer) = match self.panes.iter().flatten().next() {
            Some(other) => (other.nav.folders.clone(), other.nav.writer.clone()),
            None => (
                std::rc::Rc::new(std::cell::RefCell::new(
                    crate::navigation::folder_state::Folders::from_settings(
                        &self.launch.settings.data,
                        self.launch.settings.newest.clone(),
                    ),
                )),
                std::rc::Rc::new(std::cell::RefCell::new(
                    crate::navigation::folder_state::Writer::live(),
                )),
            ),
        };
        host.nav = crate::navigation::Navigator::new(folders, writer);
        let mut listing = host.nav.listing(&host.pane, &host.path.clone(), false);
        listing.select = host.select.clone();
        #[cfg(test)]
        let started = constructor_fixture::start_backend(&listing, cx);
        #[cfg(not(test))]
        let started = Backend::start_listing(&listing);
        match started {
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
                // Each backend watches the clipboard from its start, as Backend.qml does.
                self.clipboard.board.watch(side as u64, &crate::g2::dg1::join::PaneWire(Some(&backend)));
                host.backend = Some(backend);
            }
            Err(error) => {
                host.stopped = true;
                // The pane draws its error state, a completed surface, as a failed backend event does.
                if host.pane.fail("backend", "", "", 0, host.pane.now()) {
                    self.surfaces[side].refuse(crate::window::surfaces::Refusal {
                        operation: "backend".into(),
                        message: String::new(),
                        mode: 0,
                    });
                }
                host.changed(&self.locale);
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
        if self.defer_grid_view(view, window) {
            cx.notify();
            return;
        }
        self.apply_view(view, window, cx);
    }

    /// Lands the destination only after the Grid has complete coverage or a complete error surface.
    pub(crate) fn apply_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        for host in self.panes.iter_mut().flatten() {
            host.input.cancel(&mut host.selected, &mut host.pane.cursor);
            host.input.frame = None;
            host.input.drawn = None;
            host.input.press = None;
            host.input.pulled = None;
        }
        if self.view != view {
            self.presentation.invalidate();
        }
        let previous = self.view;
        if view != View::Dual {
            self.view = view;
        }
        self.leave_grid(previous, window);
        if view == View::Dual {
            self.request_dual(window, cx);
        } else {
            self.dual_pending = false;
            self.view = view;
            self.focus_side = 0;
            self.keyboard.context = flea::keys::Context::Listing;
            self.focus_rail(false, cx);
            if !self.card_holds_keyboard(cx) {
                window.focus(&self.focus, cx);
            }
            self.dual_cancel(cx);
        }
        self.sync_columns(window, cx);
        self.sync_trash_presentation(window, cx);
        for side in 0..2 {
            self.retick_feedback(side, window, cx);
        }
        cx.notify();
    }

    pub fn focus_pane(&mut self, side: usize, cx: &mut Context<Self>) {
        if side < 2 && (side == 0 || self.view == View::Dual) && self.panes[side].is_some() {
            self.focus_side = side;
            self.focus_rail(false, cx);
            self.keyboard.context = flea::keys::Context::Listing;
            if self.view == View::Dual {
                self.dual_focus.last_side = side;
            }
            self.remember_dual();
            cx.notify();
        }
    }

    pub fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.lifecycle.begin_close() {
            self.network_shutdown(cx);
            self.presentation.close();
            self.stop_selection_previews(window, cx);
            self.deadlines = [None, None];
            for host in self.panes.iter_mut().flatten() {
                host.transition = None;
                host.pane.swap.mark_shown(false, host.pane.now());
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

#[cfg(test)]
#[path = "state_constructor_fixture.rs"]
pub(crate) mod constructor_fixture;
