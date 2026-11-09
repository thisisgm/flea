//! The live rail join: one lazy host and one places producer per window, and the producer's requests routed to the shell.
use crate::g2::device_rail::{
    process::Runner,
    source::{Activation, Request as DeviceRequest, Source},
};
pub use crate::g2::rail_places::producer::Request;
use crate::g2::rail_places::producer::{Event, Producer};
use crate::rail::host::{self, Host};
use crate::rail::{geometry, Mode};
use crate::state::Shell;
use crate::window::dispatch::Order;
use crate::window::navigate::Step;
use flea::jsondoc::Json;
use flea::{i18n::I18n, ui_logic_rail_places::placeview::View};
use gpui::{AnyView, AppContext, Context, Entity, EventEmitter, Subscription, Window};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenClaim {
    Device,
    NetworkPending,
    Network {
        origin: u64,
        generation: Option<u64>,
    },
}

pub struct Join {
    pub host: Entity<Host>,
    pub places: Entity<Producer>,
    pub devices: Entity<Source>,
    _devices: Subscription,
    _observe: Subscription,
    _host: Subscription,
    pub(crate) focus_pending: Option<bool>,
    /// A keyboard rail open hands focus to the list only when its own accepted open lands.
    pub(crate) focus_on_open: Option<OpenClaim>,
    _trash: Subscription,
    /// The NETWORK service, card and rows, built on the first frame that can carry them.
    pub(crate) network: Option<crate::window::network::Network>,
    pub(crate) network_runner: std::rc::Rc<dyn crate::g2::device_rail::process::Runner>,
    /// The unlock card over a locked row, built on its first open; its children run on the device runner.
    pub(crate) unlock: Option<crate::window::unlock::Unlock>,
    pub(crate) device_runner: Rc<dyn Runner>,
    /// NETWORK heading plus presses already routed.
    pub(crate) add_seen: u64,
}

pub use compose::mount;
#[cfg(test)]
pub(crate) use compose::mount_with_source;

impl Shell {
    /// Polling follows the host's one gate, so a hidden or withdrawn rail asks its producer for nothing.
    pub(crate) fn rail_host(&mut self, event: host::Event, cx: &mut Context<Self>) {
        let Some(join) = &mut self.rail else {
            return;
        };
        match event {
            host::Event::Polling(polling) => {
                join.places
                    .update(cx, |producer, cx| producer.set_polling(polling, cx));
                join.devices
                    .update(cx, |source, cx| source.set_polling(polling, cx));
                // A reveal asks the plugin sections now; the devices poll above already lists the shares.
                if polling {
                    self.plugin_rail_poll(cx);
                }
            }
            host::Event::Focus(handoff) => {
                join.focus_pending = Some(handoff == crate::rail::Handoff::Rail);
                cx.notify();
            }
        }
    }

    /// A device release in flight holds the rail up, as PaneRail.qml releaseHold does, and lets go when the child ends.
    fn hold_rail_for_release(&mut self, cx: &mut Context<Self>) {
        let Some(join) = &self.rail else {
            return;
        };
        let releasing = join.devices.read(cx).devices.releasing();
        if join.host.read(cx).rail.wants().releasing != releasing {
            join.host
                .update(cx, |host, cx| host.releasing(releasing, cx));
        }
    }

    /// A row's request is applied once the frame can still run: opens route through the pane's own step.
    pub(crate) fn route_rail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reaim_file_cue(window, cx);
        self.settle_rail_drag(window, cx);
        self.reaim_rail_drag(window, cx);
        self.route_device_events(window, cx);
        self.hold_rail_for_release(cx);
        if let Some(held) = self
            .rail
            .as_mut()
            .and_then(|join| join.focus_pending.take())
        {
            self.focus_rail(held, cx);
            window.focus(
                if held {
                    &self.dual_focus.rail
                } else {
                    &self.focus
                },
                cx,
            );
            self.keyboard.context = if held {
                flea::keys::Context::Rail
            } else {
                flea::keys::Context::Listing
            };
        }
        self.follow_plugins(window, cx);
        self.plugin_place_routes(window, cx);
        self.plugin_rail_poll(cx);
        self.ensure_network(window, cx);
        let added = self.rail.as_mut().and_then(|join| {
            let model = join.places.read(cx).model.borrow().add_presses;
            let side = join.places.read(cx).side;
            (model != join.add_seen).then(|| {
                join.add_seen = model;
                side
            })
        });
        if let Some(side) = added {
            self.network_plus(side, window, cx);
        }
        let events = self
            .rail
            .as_ref()
            .map(|join| join.places.read(cx).drain_events())
            .unwrap_or_default();
        for Request { side, id, event } in events {
            match event {
                Event::Activate(entry)
                    if entry.kind == flea::ui_logic_rail_places::rail::Kind::PluginRow =>
                {
                    cx.defer_in(window, move |shell, window, cx| {
                        shell.plugin_rail_activate(side, &entry, window, cx)
                    });
                }
                Event::Activate(entry)
                    if entry.group == flea::ui_logic_rail_places::rail::Group::Network =>
                {
                    let keyboard = side == self.focus_side
                        && self.dual_focus.rail.contains_focused(window, cx)
                        && self
                            .rail
                            .as_ref()
                            .is_some_and(|join| !join.places.read(cx).model.borrow().pointer_focus);
                    if let Some(join) = &mut self.rail {
                        join.focus_on_open = keyboard.then_some(OpenClaim::NetworkPending);
                    }
                    self.network_activate(side, &entry, window, cx);
                }
                Event::Menu(entry)
                    if entry.group == flea::ui_logic_rail_places::rail::Group::Network =>
                {
                    self.network_menu(side, &entry, window, cx);
                }
                Event::Rename { entry, label }
                    if entry.group == flea::ui_logic_rail_places::rail::Group::Network =>
                {
                    let edit = crate::g2::network_shares::bookmarks::Edit::Rename {
                        uri: entry.uri.clone(),
                        label,
                    };
                    self.network_save(edit, None, None, cx);
                }
                Event::Focus => {
                    if let Some(join) = &mut self.rail {
                        join.focus_on_open = None;
                    }
                    if side == self.focus_side {
                        self.focus_rail(true, cx);
                        window.focus(&self.dual_focus.rail, cx);
                        self.keyboard.context = flea::keys::Context::Rail;
                    }
                }
                Event::Open(path) => {
                    let accepted = self
                        .step(side, Step::OpenPlace(path), window, cx)
                        .is_ok_and(|opened| opened);
                    let keyboard_open = self
                        .rail
                        .as_ref()
                        .is_some_and(|join| !join.places.read(cx).model.borrow().pointer_focus);
                    if accepted && keyboard_open && side == self.focus_side {
                        self.rail_focus(false, cx);
                        window.focus(&self.focus, cx);
                        self.keyboard.context = flea::keys::Context::Listing;
                    }
                }
                Event::Trash
                    if self.trash.is_some()
                        && side < 2
                        && self.panes[side].is_some()
                        && (side == 0 || self.view == crate::window::startup::View::Dual) =>
                {
                    self.focus_pane(side, cx);
                    self.trash_action("openTrash", window, cx);
                }
                // The device source's notify is lost inside Shell::render, so its events would wait for an unrelated frame.
                Event::Activate(entry)
                    if entry.group == flea::ui_logic_rail_places::rail::Group::Device =>
                {
                    if let Some(join) = &mut self.rail {
                        join.focus_on_open = (side == self.focus_side
                            && !join.places.read(cx).model.borrow().pointer_focus)
                            .then_some(OpenClaim::Device);
                    }
                    let event = Event::Activate(entry);
                    cx.defer_in(window, move |shell, window, cx| {
                        shell.device_request(Request { side, id, event }, window, cx)
                    });
                }
                // A remote Favorite mounts its share as a NETWORK row does, then its path opens.
                Event::Activate(entry)
                    if matches!(
                        entry.kind,
                        flea::ui_logic_rail_places::rail::Kind::Favourite
                            | flea::ui_logic_rail_places::rail::Kind::Favorite
                    ) && entry.path.contains("://")
                        && !entry.path.starts_with("file://") =>
                {
                    self.favourite_remote_activate(side, &entry, window, cx);
                }
                Event::Release(entry) => {
                    let event = Event::Release(entry);
                    cx.defer_in(window, move |shell, window, cx| {
                        shell.device_request(Request { side, id, event }, window, cx)
                    });
                }
                Event::Recent(visits) => self.recent_completed(side, visits, window, cx),
                event => self.dispatch(Order::Place(Request { side, id, event }), window, cx),
            }
        }
    }

    /// A device row's Return or release, run after the draw that drained it.
    fn device_request(&mut self, request: Request, window: &mut Window, cx: &mut Context<Self>) {
        let Request { side, id, event } = request;
        match event {
            Event::Activate(entry) => {
                let owned = self.rail.as_ref().map(|join| {
                    join.devices
                        .update(cx, |source, cx| source.activate(&entry, side, cx))
                });
                match owned {
                    Some(Activation::Handled) => {}
                    Some(Activation::Phone(phone)) => self.open_phone(side, &phone, window, cx),
                    _ => cx.emit(Request {
                        side,
                        id,
                        event: Event::Activate(entry),
                    }),
                }
            }
            Event::Release(entry) => self.release_rail_row(side, &entry, cx),
            _ => {}
        }
    }

    pub(crate) fn settle_rail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tags_sync(window, cx);
        let path = self.panes[self.focus_side]
            .as_ref()
            .map(|host| (host.path.clone(), host.recent.on));
        let Some((path, recent)) = path else {
            return;
        };
        let trash = self.trash.as_ref().is_some_and(|host| {
            host.side == self.focus_side && self.slots.covers[host.side].is_some()
        });
        if let Some(join) = &self.rail {
            if !join.host.read(cx).rail.shown() {
                return;
            }
            join.places.update(cx, |producer, cx| {
                producer.side = self.focus_side;
                producer.model.borrow_mut().corner = self.corner;
                match (trash, recent) {
                    (true, _) => producer.settle_view(&path, &View::Trash, cx),
                    (_, true) => producer.settle_view(&path, &View::Recent, cx),
                    _ => producer.settle_lit(&path, cx),
                }
            });
        }
    }
}

impl EventEmitter<Request> for Shell {}
impl EventEmitter<DeviceRequest> for Shell {}

#[path = "rail_compose.rs"]
mod compose;
#[path = "rail_keys.rs"]
mod keys;
#[path = "rail_state.rs"]
mod state;

#[cfg(test)]
#[path = "rail_tests.rs"]
mod u08;

#[cfg(test)]
#[path = "rail_devices_tests.rs"]
mod device_tests;

#[cfg(test)]
#[path = "network_readers_tests.rs"]
mod network_readers;
#[cfg(test)]
#[path = "rail_reveal_tests.rs"]
mod reveal_tests;

#[path = "../g2/device_rail/rail_events.rs"]
mod device_events;
