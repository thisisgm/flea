//! Dispatch the composed window's readers to their actual mounted producer.
use crate::{
    ipc::{server::Event, window_feedback::MenuFocus, Surface},
    state::Shell,
};
use gpui::{Context, Window};

#[path = "events_ipc_reader.rs"]
mod reader;

impl Shell {
    pub(crate) fn ipc(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            // Ahead of the cold answer, because an outside action needs a refusal it can read, never "Target not found."
            Event::Call { request, answer }
                if crate::window::external_action_ipc::is_action_call(&request) =>
            {
                if self.ipc_cold {
                    let reply = self.action_refused("menu.refusal.unavailable");
                    let _ = answer.try_send(crate::ipc::server::Answer { reply, quit: false });
                } else {
                    self.action_call(request, answer, window, cx);
                }
            }
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
                if self.network_mountinfo_call(&request, &answer, cx) {
                    return;
                }
                if let Some(reply) = self.trash_focus_call(&request, window, cx) {
                    let _ = answer.try_send(crate::ipc::server::Answer { reply, quit: false });
                    return;
                }
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
                        })
                        // TrashView takes the list stop in v0.3.12, its strip and its rows alike.
                        .or_else(|| {
                            self.trash
                                .as_ref()
                                .filter(|trash| {
                                    let pane = trash.pane.read(cx);
                                    pane.view().opened && pane.focus.contains_focused(window, cx)
                                })
                                .map(|_| "list")
                        })
                        // EY-150: Settings leaves the pane's focusView alone and its close hands the keyboard back to the list.
                        .or_else(|| self.settings.opened.then_some("list")),
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
                let trash = crate::g2::trash_browser::readers::TrashAdapter {
                    host: self.trash.as_ref(),
                    message: trash_message.as_deref(),
                    overlay: self.overlay.as_ref(),
                    cx,
                };
                let omi = crate::g2::file_ops::ipc::OmiAdapter {
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
                let favourites = crate::ipc::favourites::FavouritesAdapter {
                    host: &self.settings,
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
                let network_menu = crate::ipc::network::NetworkMenu {
                    menu: self
                        .rail
                        .as_ref()
                        .and_then(|join| join.network.as_ref())
                        .map(|network| &*network.menu),
                    catalog: &self.locale.catalog,
                    pointer: (
                        window.mouse_position().x.into(),
                        window.mouse_position().y.into(),
                    ),
                };
                let menu = crate::g2::context_menus::ipc::JoinedAdapter {
                    joined: &self.menu,
                    identity: self.menu_identity(),
                    catalog: &self.locale.catalog,
                };
                let operations = crate::g2::file_ops::ipc::LiveOperationsAdapter {
                    overlay: self.overlay.as_ref().map(|view| view.read(cx)),
                };
                let clipboard = crate::g2::file_clipboard::ipc::ClipboardAdapter {
                    board: &self.clipboard.board,
                };
                let geometry = self.geometry(window);
                let recent = crate::ipc::recent::RecentAdapter(&host.recent);
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
                let search = crate::g2::file_search::SearchAdapter {
                    search: &host.search.owner,
                };
                let query = super::super::search::QueryReader {
                    search: &host.search.owner,
                    focused: self.focus.is_focused(window) && !self.input_blocked(cx),
                };
                let network_result = crate::g2::network_shares::result::Adapter(
                    self.rail
                        .as_ref()
                        .and_then(|rail| rail.network.as_ref())
                        .map(|network| network.service.read(cx)),
                );
                let providers = crate::g2::send_share::reader::State {
                    shell: self,
                    window,
                };
                // Quick Look answers before the column, so previewState reads the card while it is open.
                let look = crate::window::quick_look::readers::Reader {
                    pdf_focused: crate::ipc::keyboard_state::pdf_focused(self, window, cx),
                    host: self.quick_look.as_ref(),
                    view: self.quick_look.as_ref().map(|host| host.view.read(cx)),
                    viewport: window.viewport_size(),
                    failure: self.quick_look_failure(cx),
                    cx,
                };
                let rename = crate::window::rename::ipc::Reader { shell: self };
                let delivery = crate::window::key_delivery::Reader {
                    shell: self,
                    window,
                    cx,
                };
                let devices = crate::ipc::devices::DeviceAdapter {
                    source: self.rail.as_ref().map(|join| join.devices.read(cx)),
                };
                let thumbnails = crate::ipc::thumbnails::Thumbnails {
                    host,
                    policy: crate::grid::element::pictures::policy::Policy::read(
                        &self.launch.settings.data,
                    ),
                    preview: preview.thumbnail_policy(host),
                };
                let preview_text = crate::ipc::preview_text::PreviewText {
                    shell: self,
                    window,
                    cx,
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
                    focus: MenuFocus::read(self, focus.view.unwrap_or(""), states.focused, window),
                    share_focused: self
                        .rail
                        .as_ref()
                        .and_then(|rail| rail.network.as_ref())
                        .is_some_and(|network| network.shares.read(cx).inside(window, cx)),
                    drop_active: self.listing_floor_active(host, &list, &states, window, cx),
                };
                let archive_preview =
                    crate::ipc::fileops_preview::PreviewAdapter { shell: self, cx };
                let column_preview =
                    crate::ipc::columns_preview::ColumnsPreview { preview: &preview };
                let glass = super::super::glass::Reader { window, cx };
                let preview_scroll = crate::ipc::preview_scroll::PreviewScroll {
                    look: look.view,
                    cx,
                };
                let preview_look = crate::ipc::preview_look::PreviewLook {
                    look: look.view,
                    cx,
                    pdf_focused: look.pdf_focused,
                };
                let preview_html = crate::ipc::preview_html::PreviewHtml {
                    shell: self,
                    window,
                    cx,
                };
                let browse_state = crate::ipc::browse_state::BrowseState {
                    shell: self,
                    window,
                    cx,
                };
                let preview_state = crate::ipc::preview_state::PreviewState {
                    shell: self,
                    window,
                    cx,
                };
                let rail_facts = crate::ipc::rail_devices::RailDeviceFacts {
                    rail: &rail,
                    devices: devices.source,
                };
                let drag_state = crate::ipc::drag_state::DragState {
                    shell: self,
                    window,
                    cx,
                };
                let adapters: [&dyn crate::ipc::Adapter; 46] = [
                    &glass,
                    &column_preview,
                    &preview_state,
                    &drag,
                    &providers,
                    &network_result,
                    &preview_scroll,
                    &look,
                    &preview_look,
                    &preview_html,
                    &thumbnails,
                    &preview_text,
                    &rename,
                    &search,
                    &query,
                    &recent,
                    &browse_state,
                    &delivery,
                    &rail_facts,
                    &focus,
                    &pane,
                    &navigation,
                    &native,
                    &dual,
                    &status,
                    &settings,
                    &favourites,
                    &trash,
                    &omi,
                    &columns,
                    &preview,
                    &grid,
                    &network_menu,
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
                    &archive_preview,
                    &drag_state,
                    &rows,
                ];
                let count = if matches!(
                    host.drawn_view,
                    Some(crate::window::startup::View::List | crate::window::startup::View::Dual)
                ) {
                    adapters.len()
                } else {
                    adapters.len() - 1
                };
                let cohesion = self.cohesion_inputs(host, &states, &rows, cx);
                let reply = if let Some(chrome) = &self.chrome.view {
                    let (path, frame) =
                        chrome.read_with(cx, |view, _| (view.path.clone(), view.frame.clone()));
                    path.read_with(cx, |path, _| {
                        let path = crate::ipc::path::PathAdapter { view: path };
                        let frame = frame.borrow();
                        let chrome = crate::ipc::chrome::ChromeAdapter { frame: &frame };
                        let laid = cohesion.laid.as_ref().map(|laid| laid.borrow());
                        let cohesion = cohesion.adapter(&frame, &path, laid.as_deref());
                        let mut mounted = adapters[..count].to_vec();
                        mounted.extend([&path as &dyn crate::ipc::Adapter, &chrome, &tabs]);
                        mounted.push(&cohesion);
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
