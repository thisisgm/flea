//! The live network join: PL2's mount service, its card, its share browser and the rail's NETWORK rows, owned per window.
use crate::g2::op1::palette::CardPalette;
use crate::g2::pl1::listing::Listing;
use crate::g2::pl1::process::Runner;
use crate::g2::pl1::secret::Secret;
use crate::g2::pl2::{
    bookmarks,
    browser::{Activated, Closed, ShareView},
    dialog, menus,
    mount::Effect,
    provider::ProviderStatus,
    service::{Service, ServiceEvent},
    view::{NetworkEvent, NetworkView},
};
use crate::list::ListState;
use crate::state::Shell;
use flea::jsondoc::Json;
use flea::ui_logic_pl1::RailEntry;
use flea::ui_logic_pl4::order::{Heading, RailSection};
use flea::ui_logic_pl4::rail::{self as rail, Group, Kind};
use gpui::{AppContext, Context, Entity, Subscription, Task, Window};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::{Rc, Weak};

/// The pane a request came from: its side and that pane's own list, so a reply never lands on a pane opened later in its place.
struct Origin {
    token: u64,
    side: usize,
    list: Weak<RefCell<ListState>>,
}

pub struct Network {
    pub(crate) sources: cloud::Sources,
    pub service: Entity<Service>,
    pub card: Entity<NetworkView>,
    pub shares: Entity<ShareView>,
    pub rows: Vec<RailEntry>,
    /// Bumped by each rebuild and each saved bookmark write, so a read a newer one superseded never publishes its rows.
    pub rows_generation: u64,
    /// True once rows were rebuilt from a gio listing and a mountinfo read.
    pub built: bool,
    pub listing: Listing,
    listing_generation: u64,
    /// The last mountinfo read was non-empty, so networkBuilt can tell a built group from an unbuilt one.
    pub mountinfo_ready: bool,
    pub menu: Rc<RefCell<crate::g2::mn1::menu::Menu>>,
    pub menu_row: Option<(RailEntry, u64)>,
    /// The NETWORK plus's hosts menu while it holds the shared menu, and the last request id it used.
    pub(crate) nearby: Option<nearby::Nearby>,
    pub(crate) nearby_serial: usize,
    pub(crate) runner: Rc<dyn Runner>,
    pub(crate) bookmarks: PathBuf,
    origins: Vec<Origin>,
    next_origin: u64,
    return_focus: Option<(usize, bool)>,
    connect_errors: [Vec<String>; 2],
    tasks: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

/// Production runs real children; unit fixtures get a runner whose jobs never start unless a test injects its own.
pub(crate) fn default_runner() -> Rc<dyn Runner> {
    #[cfg(not(test))]
    return Rc::new(crate::g2::pl1::process::ThreadRunner);
    #[cfg(test)]
    return Rc::new(Inert);
}

#[cfg(test)]
struct Inert;
#[cfg(test)]
impl Runner for Inert {
    fn start(&self, _: crate::g2::pl1::process::Job) -> crate::g2::pl1::process::Handle {
        crate::g2::pl1::process::Handle::detached().0
    }
}

/// The GTK bookmarks file saved places live in; unit fixtures get no path, so a write refuses unless a test names a sandbox file.
fn bookmarks_path() -> PathBuf {
    if cfg!(test) {
        return PathBuf::new();
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".config"))
        .join("gtk-3.0/bookmarks")
}

pub(crate) fn shown(state: &Json) -> bool {
    state
        .get("places")
        .and_then(|places| places.get("showNetwork"))
        .and_then(Json::as_bool)
        != Some(false)
}

impl Shell {
    /// Builds the service, card and browser once a frame can carry them; nothing runs until a row, key or poll asks.
    pub(crate) fn ensure_network(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(join) = &self.rail else {
            return;
        };
        if join.network.is_some() {
            return;
        }
        let runner = join.network_runner.clone();
        let devices = join.devices.clone();
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let palette = CardPalette::from_theme(&window.text_style(), theme, self.corner);
        let catalog = self.locale.catalog.clone();
        let service = cx.new(|_| {
            Service::new(
                runner.clone(),
                std::env::var("XDG_RUNTIME_DIR").unwrap_or_default(),
                std::env::var("FLEA_GVFS_FUSE").unwrap_or_default(),
            )
        });
        let card = cx.new(|cx| NetworkView::new(catalog.clone(), palette, cx));
        let shares = cx.new(|cx| ShareView::new(catalog, palette, cx));
        let subscriptions = vec![
            cx.observe(&devices, |shell, _, cx| shell.network_sync_mounts(cx)),
            cx.subscribe_in(
                &cx.entity(),
                window,
                |shell, _, event: &crate::path::join::NetworkPath, window, cx| {
                    shell.network_address(event.side, &event.uri, window, cx)
                },
            ),
            cx.subscribe_in(
                &service,
                window,
                |shell, _, event: &ServiceEvent, window, cx| {
                    shell.network_service(&event.0, window, cx)
                },
            ),
            cx.subscribe(&service, |shell, _, status: &ProviderStatus, cx| {
                if let Some(network) = shell.network() {
                    let card = network.card.clone();
                    card.update(cx, |card, cx| card.provider_status(status, cx));
                }
            }),
            cx.subscribe_in(
                &card,
                window,
                |shell, _, event: &NetworkEvent, window, cx| {
                    shell.network_card(&event.0, window, cx)
                },
            ),
            cx.subscribe(&shares, |shell, _, chosen: &Activated, cx| {
                let Some(network) = shell.network() else {
                    return;
                };
                let service = network.service.clone();
                let (uri, label, origin) = (chosen.0.clone(), chosen.1.clone(), chosen.2);
                service.update(cx, |service, cx| {
                    service.open_child_share(uri, label, origin, cx)
                });
            }),
            cx.subscribe_in(&shares, window, |shell, _, closed: &Closed, window, cx| {
                if let Some(network) = shell.network() {
                    let service = network.service.clone();
                    service.update(cx, |service, _| service.close_browser(closed.0));
                }
                shell.network_restore_focus(window, cx);
            }),
        ];
        if let Some(join) = &mut self.rail {
            join.network = Some(Network {
                sources: cloud::Sources::default(),
                service,
                card,
                shares,
                rows: Vec::new(),
                rows_generation: 0,
                built: false,
                listing: Listing::new(),
                listing_generation: 0,
                mountinfo_ready: false,
                menu: Rc::default(),
                menu_row: None,
                nearby: None,
                nearby_serial: 0,
                runner,
                bookmarks: bookmarks_path(),
                origins: Vec::new(),
                next_origin: 0,
                return_focus: None,
                connect_errors: Default::default(),
                tasks: Vec::new(),
                _subscriptions: subscriptions,
            });
            self.chrome.network = true;
        }
        self.network_sync_mounts(cx);
    }

    /// The NETWORK group is built and shown, so an empty group is an answer rather than the unbuilt state.
    pub(crate) fn network_built(&self) -> bool {
        shown(&self.launch.settings.data)
            && self
                .rail
                .as_ref()
                .and_then(|join| join.network.as_ref())
                .is_some_and(|network| network.built)
    }

    pub(crate) fn network(&mut self) -> Option<&mut Network> {
        self.rail.as_mut().and_then(|join| join.network.as_mut())
    }

    /// A new token for the pane on this side; the eight newest stay answerable.
    pub(crate) fn network_origin(&mut self, side: usize) -> u64 {
        let list = self.panes[side]
            .as_ref()
            .map(|host| Rc::downgrade(&host.list))
            .unwrap_or_default();
        let Some(network) = self.network() else {
            return 0;
        };
        network.next_origin += 1;
        let token = network.next_origin;
        network.origins.push(Origin { token, side, list });
        if network.origins.len() > 8 {
            network.origins.remove(0);
        }
        token
    }

    /// The side a token still names: the same pane, alive and shown; anything else is a stale reply.
    pub(crate) fn network_side(&self, token: u64) -> Option<usize> {
        let side = self.network_origin_side(token)?;
        (side == 0 || self.view == crate::window::startup::View::Dual).then_some(side)
    }

    /// Readers retain a hidden pane's origin while navigation still requires network_side's visible owner.
    pub(crate) fn network_origin_side(&self, token: u64) -> Option<usize> {
        let origin = self
            .rail
            .as_ref()?
            .network
            .as_ref()?
            .origins
            .iter()
            .find(|origin| origin.token == token)?;
        let host = self.panes.get(origin.side)?.as_ref()?;
        origin
            .list
            .upgrade()
            .is_some_and(|list| Rc::ptr_eq(&list, &host.list))
            .then_some(origin.side)
    }

    /// The pane's state surface, withheld while the share browser covers it, as WindowBody.qml hides it in v0.3.12.
    pub(crate) fn pane_state_view(&self, side: usize, cx: &gpui::App) -> Option<gpui::AnyView> {
        let Some(network) = self.rail.as_ref().and_then(|join| join.network.as_ref()) else {
            return self.slots.states[side].clone();
        };
        let browser = network.shares.read(cx);
        let covered = browser.visible() && self.network_side(browser.model.origin) == Some(side);
        self.slots.states[side].clone().filter(|_| !covered)
    }

    /// Opens Add a Network Share for this pane; the shipped addNetwork key and the rail's plus both land here.
    pub(crate) fn network_add(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_network(window, cx);
        let origin = self.network_origin(side);
        self.network_remember_focus();
        let Some(network) = self.network() else {
            return;
        };
        let card = network.card.clone();
        card.update(cx, |card, cx| card.open_for(origin, window, cx));
        cx.notify();
    }

    pub(crate) fn network_holds_keyboard(&self, cx: &gpui::App) -> bool {
        self.rail
            .as_ref()
            .and_then(|join| join.network.as_ref())
            .is_some_and(|network| {
                network.card.read(cx).model.opened || network.menu.borrow().opened()
            })
    }

    /// The join's own row for a rail row, so release reads the row's fstype and mount path, never a rebuilt copy.
    pub(crate) fn network_row(&mut self, entry: &rail::RailEntry) -> Option<RailEntry> {
        self.network()?
            .rows
            .iter()
            .find(|row| row.uri == entry.uri && row.path == entry.path)
            .cloned()
    }

    /// A NETWORK row opens through the service: a mounted path directly, anything else as one single-flight mount.
    pub(crate) fn network_activate(
        &mut self,
        side: usize,
        entry: &rail::RailEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_network(window, cx);
        let Some(row) = self.network_row(entry) else {
            self.bind_rail_network_open(None, None);
            return;
        };
        let origin = self.network_origin(side);
        self.bind_rail_network_open(Some(origin), None);
        match menus::activate(&row, origin) {
            menus::Open::Direct { path, .. } => {
                let opened = self.open_rail_place(side, &path, window, cx);
                self.land_rail_network_open(origin, side, opened, window, cx);
            }
            menus::Open::Remote(request) => {
                let Some(network) = self.network() else {
                    return;
                };
                let service = network.service.clone();
                let generation = service.update(cx, |service, cx| {
                    let now = service.now(cx);
                    service.open_share(request, now, cx);
                    service.mounts.generation
                });
                self.bind_rail_network_open(Some(origin), Some(generation));
            }
        }
    }

    /// A remote Favorite opens like its NETWORK share: one mount request, then the path it reaches opens (Places.openEntry).
    pub(crate) fn favourite_remote_activate(
        &mut self,
        side: usize,
        entry: &rail::RailEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_network(window, cx);
        let origin = self.network_origin(side);
        let Some(network) = self.network() else {
            return;
        };
        let service = network.service.clone();
        let request = crate::g2::pl2::mount::Request {
            id: 0,
            uri: entry.path.clone(),
            label: entry.label.clone(),
            origin,
            mounted: entry.mounted == Some(true),
            authenticated: false,
            password: crate::g2::pl1::secret::Secret::new(),
        };
        service.update(cx, |service, cx| {
            let now = service.now(cx);
            service.open_share(request, now, cx)
        });
    }

    /// Unmount releases the row's own identity: FUSE by its path, a lent kernel mount by its exact file URI.
    pub(crate) fn network_unmount(&mut self, row: &RailEntry, origin: u64, cx: &mut Context<Self>) {
        let Some(network) = self.network() else {
            return;
        };
        let service = network.service.clone();
        let listing = network.listing.text.clone();
        service.update(cx, |service, cx| service.unmount(row, &listing, origin, cx));
    }

    /// Window close stops every child and timer the network join started.
    pub(crate) fn network_shutdown(&mut self, cx: &mut Context<Self>) {
        let Some(network) = self.network() else {
            return;
        };
        let service = network.service.clone();
        let done = service.update(cx, |service, cx| service.shutdown(cx));
        if let Some(network) = self.network() {
            network.tasks.push(done);
        }
    }
}

#[path = "network_effects.rs"]
mod effects;
#[path = "network_focus.rs"]
mod focus;
#[path = "network_menu.rs"]
mod menu;
#[path = "network_nearby.rs"]
pub(crate) mod nearby;
#[path = "network_path.rs"]
mod path;
#[path = "network_rows.rs"]
mod rows;

#[path = "network_cloud.rs"]
mod cloud;

#[cfg(test)]
#[path = "network_tests.rs"]
pub(crate) mod tests;

#[path = "network_section.rs"]
mod section_rows;
pub use section_rows::section;
