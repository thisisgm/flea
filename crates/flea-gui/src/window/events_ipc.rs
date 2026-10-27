//! Dispatch the composed window's readers to their actual mounted producer.
use crate::{
    ipc::{server::Event, Surface},
    state::Shell,
};
use gpui::{Context, Window};

impl Shell {
    /// The first listing answered or gave up waiting, so calls from here on reach the window's readers.
    pub(crate) fn ensure_ipc_reader(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.ipc_cold = false;
        self.start_ipc_reader(window, cx);
    }

    pub(crate) fn start_ipc_reader(&mut self, window: &Window, cx: &mut Context<Self>) {
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

    pub(crate) fn ipc(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::Call { answer, .. } if self.ipc_cold => {
                let _ = answer.try_send(crate::ipc::server::Answer {
                    reply: flea::ipc::registry::no_target(),
                    quit: false,
                });
            }
            Event::Call { request, answer } => {
                let (request, answer) = match self.visual_call(request, answer, window, cx) {
                    Some(call) => call,
                    None => return,
                };
                let Some(host) = &self.panes[self.focus_side] else {
                    return;
                };
                if crate::ipc::tabs::taken(&request).is_some() {
                    self.tab_ack_call(&request, &answer, window, cx);
                    return;
                }
                let surface = Surface {
                    path: &host.path,
                    total: host.pane.held.total,
                    cursor: host.pane.cursor,
                    selected: &host.selected,
                };
                let grid = crate::ipc::grid_rows::MountedGrid {
                    grid: host.grid.as_ref(),
                    active: host.drawn_view == Some(crate::window::startup::View::Grid),
                };
                let chrome_glyph = self.chrome.view.as_ref().and_then(|chrome| {
                    let frame = chrome.read(cx).frame.borrow();
                    frame
                        .controls
                        .iter()
                        .position(|control| control.focused)
                        .map(|at| crate::path::chrome::GLYPHS[at])
                });
                let focus = crate::ipc::focus::FocusAdapter {
                    view: Some(self.focus_view(window, cx))
                        .filter(|view| !view.is_empty())
                        .or_else(|| {
                            host.preview
                                .as_ref()
                                .filter(|preview| {
                                    preview.read_with(cx, |preview, cx| {
                                        preview.visible
                                            && preview.focus.contains_focused(window, cx)
                                    })
                                })
                                .map(|_| "list")
                        }),
                    input: &host.input,
                    chrome_glyph,
                };
                let pane = crate::ipc::pane::PaneAdapter {
                    pane: &host.pane,
                    catalog: &self.locale.catalog,
                };
                let navigation = crate::ipc::navigation::NavigationAdapter {
                    navigator: &host.nav,
                };
                let home_label = self.locale.catalog.tr("tabs.home");
                let tabs = self.tab_adapter(&home_label);
                let pane = crate::ipc::tabs::SwapWithStrip {
                    pane: &pane,
                    strip: self.strip.frames.borrow().json(),
                };
                let trash_message = self.trash_message();
                let trash = crate::g2::pl3::readers::TrashAdapter {
                    host: self.trash.as_ref(),
                    message: trash_message.as_deref(),
                    overlay: self.overlay.as_ref(),
                    cx,
                };
                let omi = crate::g2::op1::ipc::OmiAdapter {
                    overlay: self.overlay.as_ref().map(|overlay| overlay.read(cx)),
                };
                let columns = crate::window::columns::readers::Reader {
                    host,
                    visible: host.drawn_view == Some(crate::window::startup::View::Columns),
                    cx,
                };
                let preview_active = host.drawn_view == Some(crate::window::startup::View::Columns)
                    && host.columns.as_ref().is_some_and(|columns| {
                        columns.preview_hosted
                            && columns
                                .area
                                .tracks()
                                .iter()
                                .any(|t| matches!(t.rows, crate::columns::TrackRows::Preview))
                    });
                let preview = crate::window::selection_preview::readers::Reader {
                    view: host.preview.as_ref().map(|view| view.read(cx)),
                    window,
                    active: preview_active,
                    cx,
                };
                let settings = crate::ipc::settings::SettingsAdapter {
                    host: &self.settings,
                    document: &self.launch.settings.data,
                    hidden: host.nav.folders.borrow().hidden,
                    cx,
                };
                let status = crate::ipc::status::StatusAdapter { shell: self, cx };
                let dual = self.dual_adapter(window, cx);
                let drag = crate::ipc::dual::DragGeometry { shell: self };
                let list = host.list.borrow();
                let native = crate::ipc::native_window::NativeWindowAdapter {
                    list: &list,
                    theme: cx
                        .try_global::<crate::palette::Theme>()
                        .copied()
                        .unwrap_or_default(),
                    parsed: cx
                        .try_global::<crate::palette::ParsedTheme>()
                        .copied()
                        .unwrap_or_default(),
                    viewport: window.viewport_size(),
                    corner: self.corner,
                };
                let states = self.list_states(self.focus_side, window, cx);
                let view = crate::ipc::view::ViewAdapter {
                    list: &list,
                    mode: host.drawn_view,
                    shown: host.drawn_list,
                };
                let view = view.mounted(host);
                let rows = crate::ipc::rows::RowsAdapter {
                    list: &list,
                    held: &host.pane.held,
                    states: &states,
                };
                let menu = crate::g2::mn1::ipc::JoinedAdapter {
                    joined: &self.menu,
                    identity: self.menu_identity(),
                    catalog: &self.locale.catalog,
                };
                let operations = crate::g2::op1::ipc::LiveOperationsAdapter {
                    overlay: self.overlay.as_ref().map(|view| view.read(cx)),
                };
                let clipboard = crate::g2::dg1::ipc::ClipboardAdapter {
                    board: &self.clipboard.board,
                };
                let geometry = self.geometry(window);
                let shown = crate::window::network::shown(&self.launch.settings.data);
                let network = self
                    .rail
                    .as_ref()
                    .and_then(|join| join.network.as_ref())
                    .map(|network| crate::ipc::rail::NetworkBuilt {
                        shown,
                        present: true,
                        listed: network.listing.answered,
                        mountinfo: network.mountinfo_ready,
                    })
                    .unwrap_or(crate::ipc::rail::NetworkBuilt {
                        shown,
                        ..Default::default()
                    });
                let rail = crate::ipc::rail::RailAdapter {
                    host: self.rail.as_ref().map(|join| join.host.read(cx)),
                    producer: self.rail.as_ref().map(|join| join.places.read(cx)),
                    geometry: &geometry,
                    focus_side: self.focus_side,
                    now: cx.background_executor().now(),
                    network,
                };
                let card = self
                    .rail
                    .as_ref()
                    .and_then(|join| join.network.as_ref())
                    .map(|network| network.card.read(cx));
                let network = crate::ipc::network::NetworkAdapter {
                    card,
                    inside: card.is_some_and(|card| card.holds_focus(window, cx)),
                    built: self.network_built(),
                    rail: &rail,
                };
                let search = crate::g2::sr1::SearchAdapter {
                    search: &host.search.owner,
                };
                let query = super::super::search::QueryReader {
                    search: &host.search.owner,
                    focused: self.focus.is_focused(window) && !self.input_blocked(cx),
                };
                let network_result = crate::g2::pl2::result::Adapter(
                    self.rail
                        .as_ref()
                        .and_then(|rail| rail.network.as_ref())
                        .map(|network| network.service.read(cx)),
                );
                let providers = crate::g2::sd1::reader::State {
                    shell: self,
                    window,
                };
                // Quick Look answers before the column, so previewState reads the card while it is open.
                let look = crate::window::quick_look::readers::Reader {
                    host: self.quick_look.as_ref(),
                    view: self.quick_look.as_ref().map(|host| host.view.read(cx)),
                    viewport: window.viewport_size(),
                    failure: self.quick_look_failure(cx),
                    cx,
                };
                let rename = crate::window::rename::ipc::Reader { shell: self };
                let delivery = crate::window::key_delivery::Reader {
                    shell: self,
                    preview_active,
                    window,
                    cx,
                };
                let devices = crate::ipc::devices::DeviceAdapter {
                    source: self.rail.as_ref().map(|join| join.devices.read(cx)),
                };
                let thumbnails = crate::ipc::thumbnails::Thumbnails {
                    host,
                    mode: crate::grid::element::pictures::policy::Policy::read(
                        &self.launch.settings.data,
                    )
                    .mode,
                };
                let archive_cards = crate::ipc::archive_cards::ArchiveAdapter { shell: self };
                let output_cards = crate::ipc::output_cards::OutputAdapter { shell: self, cx };
                let dialog_cards = crate::ipc::dialog_cards::DialogAdapter {
                    shell: self,
                    cx,
                    input_ready: states.focused && !host.pane.busy(),
                };
                let feedback = crate::ipc::window_feedback::FeedbackAdapter {
                    shell: self,
                    cx,
                    view: focus.view.unwrap_or(""),
                    pane_focused: self.focus.is_focused(window),
                    list_focused: states.focused,
                    drop_active: states.drop_target.is_some(),
                };
                let column_preview =
                    crate::ipc::columns_preview::ColumnsPreview { preview: &preview };
                let glass = super::super::glass::Reader { window, cx };
                let adapters: [&dyn crate::ipc::Adapter; 35] = [
                    &glass,
                    &column_preview,
                    &drag,
                    &providers,
                    &network_result,
                    &thumbnails,
                    &look,
                    &rename,
                    &search,
                    &query,
                    &delivery,
                    &rail,
                    &focus,
                    &pane,
                    &navigation,
                    &native,
                    &dual,
                    &status,
                    &settings,
                    &trash,
                    &omi,
                    &columns,
                    &preview,
                    &grid,
                    &menu,
                    &view,
                    &operations,
                    &devices,
                    &clipboard,
                    &network,
                    &archive_cards,
                    &output_cards,
                    &dialog_cards,
                    &feedback,
                    &rows,
                ];
                let count = if matches!(
                    host.drawn_view,
                    Some(crate::window::startup::View::List | crate::window::startup::View::Dual)
                ) {
                    35
                } else {
                    34
                };
                let reply = if let Some(chrome) = &self.chrome.view {
                    let (path, frame) =
                        chrome.read_with(cx, |view, _| (view.path.clone(), view.frame.clone()));
                    path.read_with(cx, |path, _| {
                        let path = crate::ipc::path::PathAdapter { view: path };
                        let frame = frame.borrow();
                        let chrome = crate::ipc::chrome::ChromeAdapter { frame: &frame };
                        let mut mounted = adapters[..count].to_vec();
                        mounted.extend([&path as &dyn crate::ipc::Adapter, &chrome, &tabs]);
                        surface.dispatch_to(request, &mounted)
                    })
                } else {
                    let mut mounted = adapters[..count].to_vec();
                    mounted.push(&tabs);
                    surface.dispatch_to(request, &mounted)
                };
                let _ = answer.try_send(reply);
            }
            Event::Quit => {
                self.close(window, cx);
            }
        }
    }
}
