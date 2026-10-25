//! The live rail join: one lazy host and one places producer per window, and the producer's requests routed to the shell.
use crate::g2::pl1::{
    process::Runner,
    source::{Activation, Request as DeviceRequest, Source},
};
pub use crate::g2::pl4::producer::Request;
use crate::g2::pl4::producer::{Event, Producer};
use crate::rail::host::{self, Host};
use crate::rail::{geometry, Mode};
use crate::state::Shell;
use crate::window::dispatch::Order;
use crate::window::navigate::Step;
use flea::i18n::I18n;
use flea::jsondoc::Json;
use gpui::{
    AnyView, AppContext, Context, Entity, EventEmitter, Subscription, Window,
};
use std::rc::Rc;
use std::sync::Arc;

pub struct Join {
    pub host: Entity<Host>,
    pub places: Entity<Producer>,
    pub devices: Entity<Source>,
    _devices: Subscription,
    _observe: Subscription,
    _host: Subscription,
    pub(crate) focus_pending: Option<bool>,
    _trash: Subscription,
    /// The NETWORK service, card and rows, built on the first frame that can carry them.
    pub(crate) network: Option<crate::window::network::Network>,
    pub(crate) network_runner: std::rc::Rc<dyn crate::g2::pl1::process::Runner>,
    /// The unlock card over a locked row, built on its first open; its children run on the device runner.
    pub(crate) unlock: Option<crate::window::unlock::Unlock>,
    pub(crate) device_runner: Rc<dyn Runner>,
    /// NETWORK heading plus presses already routed.
    pub(crate) add_seen: u64,
}

/// Builds the producer from Settings.data and the host over it; the host is the only lazy owner.
pub fn mount(catalog: Arc<I18n>, state: &Json, side: usize, cx: &mut Context<Shell>) -> Join {
    #[cfg(not(test))]
    let (runner, hostname) = (Rc::new(crate::g2::pl1::process::ThreadRunner), None);
    #[cfg(test)]
    let (runner, hostname) = crate::g2::pl1::source::fixture::services(cx);
    mount_with_source(catalog, state, side, runner, hostname, cx)
}

fn mount_with_source(
    catalog: Arc<I18n>,
    state: &Json,
    side: usize,
    runner: Rc<dyn Runner>,
    hostname: Option<String>,
    cx: &mut Context<Shell>,
) -> Join {
    let places = state.get("places").unwrap_or(&Json::Null);
    let mode = Mode::from_places(places);
    let width = geometry::width_stop(places.get("sidebarWidth").and_then(Json::as_f64));
    let producer = cx.new(|cx| {
        Producer::new(
            Arc::clone(&catalog),
            state,
            crate::window::startup::binary(),
            cx,
        )
    });
    producer.update(cx, |producer, _| {
        producer.polling = mode == Mode::Docked;
        producer.side = side;
    });
    let observe = cx.observe(&producer, |_, _, cx| cx.notify());
    let device_runner = runner.clone();
    let devices = cx.new(|_| Source::new(runner, hostname, state));
    let device_observe = cx.observe(&devices, |shell, _, cx| {
        shell.rail_tail(cx);
        shell.unlock_rail_changed(cx);
        cx.notify();
    });
    devices.update(cx, |source, cx| {
        source.set_polling(mode == Mode::Docked, cx)
    });
    let built = producer.clone();
    let host = cx.new(|cx| {
        Host::new(
            mode,
            width,
            Box::new(move |_, _| AnyView::from(built.clone())),
            cx,
        )
    });
    let host_subscription = cx.subscribe(&host, |shell, _, event, cx| {
        shell.rail_host(*event, cx);
    });
    let owner = cx.entity();
    let trash_subscription = cx.subscribe(
        &owner,
        |shell, _, event: &crate::g2::pl3::join::TrashEvent, cx| match event {
            crate::g2::pl3::join::TrashEvent::Count(count) => {
                shell.rail_trash_count(*count as u64, cx);
            }
            crate::g2::pl3::join::TrashEvent::FocusRail => {
                shell.rail_focus(true, cx);
            }
        },
    );
    Join {
        host,
        places: producer,
        devices,
        _devices: device_observe,
        _observe: observe,
        _host: host_subscription,
        focus_pending: None,
        _trash: trash_subscription,
        network: None,
        network_runner: crate::window::network::default_runner(),
        unlock: None,
        device_runner,
        add_seen: 0,
    }
}

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
            }
            host::Event::Focus(handoff) => {
                join.focus_pending = Some(handoff == crate::rail::Handoff::Rail);
                cx.notify();
            }
        }
    }

    /// A row's request is applied once the frame can still run: opens route through the pane's own step.
    pub(crate) fn route_rail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.route_device_events(window, cx);
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
            self.network_add(side, window, cx);
        }
        let events = self
            .rail
            .as_ref()
            .map(|join| join.places.read(cx).drain_events())
            .unwrap_or_default();
        for Request { side, id, event } in events {
            match event {
                Event::Activate(entry)
                    if entry.group == flea::ui_logic_pl4::rail::Group::Network =>
                {
                    self.network_activate(side, &entry, window, cx);
                }
                Event::Menu(entry) if entry.group == flea::ui_logic_pl4::rail::Group::Network => {
                    self.network_menu(side, &entry, window, cx);
                }
                Event::Rename { entry, label }
                    if entry.group == flea::ui_logic_pl4::rail::Group::Network =>
                {
                    let edit = crate::g2::pl2::bookmarks::Edit::Rename {
                        uri: entry.uri.clone(),
                        label,
                    };
                    self.network_save(edit, None, cx);
                }
                Event::Focus => {
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
                Event::Activate(entry)
                    if entry.group == flea::ui_logic_pl4::rail::Group::Device =>
                {
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
                event => self.dispatch(Order::Place(Request { side, id, event }), window, cx),
            }
        }
    }

    pub(crate) fn settle_rail(&mut self, cx: &mut Context<Self>) {
        let path = self.panes[self.focus_side]
            .as_ref()
            .map(|host| host.path.clone());
        let Some(path) = path else {
            return;
        };
        if let Some(join) = &self.rail {
            if !join.host.read(cx).rail.shown() {
                return;
            }
            join.places.update(cx, |producer, cx| {
                producer.side = self.focus_side;
                producer.model.borrow_mut().corner = self.corner;
                producer.settle_lit(&path, cx);
            });
        }
    }
}

impl EventEmitter<Request> for Shell {}
impl EventEmitter<DeviceRequest> for Shell {}

impl Shell {
    pub fn rail_focus(&mut self, focused: bool, cx: &mut Context<Self>) {
        let Some(join) = &self.rail else {
            return;
        };
        if focused && join.host.read(cx).rail.mode() == Mode::Hidden {
            return;
        }
        if let Some(join) = &mut self.rail {
            join.focus_pending = focused.then_some(true);
        }
        let join = self.rail.as_ref().expect("mounted rail");
        join.host.update(cx, |host, cx| host.focus(focused, cx));
        join.places.update(cx, |producer, cx| {
            producer.side = self.focus_side;
            producer.model.borrow_mut().focused = focused;
            if !focused {
                producer.model.borrow_mut().mark_focused = false;
                producer.cancel_rename(cx);
            }
            cx.notify();
        });
    }

    /// U32 owns the monitor and calls this with each accepted count; U08 only paints it.
    pub fn rail_trash_count(&mut self, count: u64, cx: &mut Context<Self>) {
        if let Some(join) = &self.rail {
            join.places.update(cx, |p, cx| p.trash_count(count, cx));
        }
    }

    /// U11's accepted document updates both placement and content atomically.
    pub fn rail_state(&mut self, state: &Json, cx: &mut Context<Self>) {
        self.launch.settings.data = state.clone();
        self.path_favourites(state, cx);
        if let Some(join) = &self.rail {
            let places = state.get("places").unwrap_or(&Json::Null);
            join.host.update(cx, |host, cx| {
                host.set_width(
                    geometry::width_stop(places.get("sidebarWidth").and_then(Json::as_f64)),
                    cx,
                );
                host.set_mode(Mode::from_places(places), cx);
            });
            join.devices
                .update(cx, |source, cx| source.adopt_state(state, cx));
            join.places
                .update(cx, |producer, cx| producer.adopt_state(state, cx));
        }
        self.rail_tail(cx);
        cx.notify();
    }
}

#[path = "rail_compose.rs"]
mod compose;
#[path = "rail_keys.rs"]
mod keys;

#[cfg(test)]
#[path = "rail_tests.rs"]
mod u08;

#[cfg(test)]
#[path = "rail_devices_tests.rs"]
mod device_tests;

#[path = "../g2/pl1/rail_events.rs"]
mod device_events;
