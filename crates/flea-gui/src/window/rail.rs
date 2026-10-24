//! The live rail join: one lazy host and one places producer per window, and the producer's requests routed to the shell.
use crate::g2::pl1::{
    process::Runner,
    source::{Request as DeviceRequest, Source},
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
    AnyView, AppContext, Context, Entity, EventEmitter, KeyDownEvent, Subscription, Window,
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
}

/// Builds the producer from Settings.data and the host over it; the host is the only lazy owner.
pub fn mount(catalog: Arc<I18n>, state: &Json, side: usize, cx: &mut Context<Shell>) -> Join {
    #[cfg(not(test))]
    let runner = Rc::new(crate::g2::pl1::process::ThreadRunner);
    #[cfg(test)]
    let runner = Rc::new(crate::g2::pl1::source::EmptyRunner);
    mount_with_source(catalog, state, side, runner, None, cx)
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
    let devices = cx.new(|_| Source::new(runner, hostname, state));
    let target = producer.clone();
    let device_observe = cx.observe(&devices, move |_, source, cx| {
        let sections = source.read(cx).sections.clone();
        target.update(cx, |producer, cx| producer.set_tail(sections, cx));
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
        let events = self
            .rail
            .as_ref()
            .map(|join| join.places.read(cx).drain_events())
            .unwrap_or_default();
        for Request { side, id, event } in events {
            match event {
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
                    if let Some(join) = &self.rail {
                        join.devices
                            .update(cx, |source, cx| source.activate(&entry, side, cx));
                    }
                }
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
        if let Some(join) = &self.rail {
            let places = state.get("places").unwrap_or(&Json::Null);
            join.host.update(cx, |host, cx| {
                host.set_width(
                    geometry::width_stop(places.get("sidebarWidth").and_then(Json::as_f64)),
                    cx,
                );
                host.set_mode(Mode::from_places(places), cx);
            });
            let tail = join.devices.update(cx, |source, cx| {
                source.adopt_state(state, cx);
                source.sections.clone()
            });
            join.places.update(cx, |producer, cx| {
                producer.set_tail(tail, cx);
                producer.adopt_state(state, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn rail_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        use super::input::{event_input, Dispatch};
        use flea::keys::{self, sequence::Seat, Frontend, Match};
        let Some(join) = &self.rail else {
            return None;
        };
        let focused =
            self.dual_focus.rail.contains_focused(window, cx) || join.focus_pending == Some(true);
        let producer = join.places.clone();
        if focused && producer.read(cx).rename_index().is_some() {
            producer.update(cx, |producer, _| producer.side = self.focus_side);
            return Some(producer.update(cx, |producer, cx| producer.rename_key(event, cx)));
        }
        let name = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(Json::as_str)
            .unwrap_or("default");
        let action = keys::lookup(
            name,
            self.keyboard.context,
            Frontend::Gui,
            event_input(event, window.modifiers()),
        );
        if action == Match::Action("sidebar") {
            if join.host.read(cx).rail.mode() == Mode::Overlay {
                return Some(true);
            }
            let hide = join.host.read(cx).rail.mode() != Mode::Hidden;
            let patch = Json::Obj(vec![(
                "places".into(),
                Json::Obj(vec![(
                    "rail".into(),
                    Json::Str(if hide { "hidden" } else { "shown" }.into()),
                )]),
            )]);
            self.change_settings(patch, window, cx);
            return Some(true);
        }
        if matches!(action, Match::Action(name) if matches!(name,
            "focusNext" | "focusPrevious" | "focusPreview" | "keymapSheet" |
            "pathBar" | "windowNew" | "reload" | "openTerminal" | "settings" |
            "copydirpath") || name.starts_with("tab") || name.starts_with("textSize"))
        {
            // U03/U26 retain the registered stop order and consecutive dual pane stops.
            return None;
        }
        if !focused {
            return None;
        }
        producer.update(cx, |producer, _| producer.side = self.focus_side);
        let seat = Seat {
            focus: 2,
            cursor: producer.read(cx).cursor().unwrap_or(0),
            ..Seat::default()
        };
        if let Dispatch::Action(action) =
            self.keyboard.resolve(name, event, window.modifiers(), seat)
        {
            if action == "escape" {
                self.rail_focus(false, cx);
                window.focus(&self.focus, cx);
                self.keyboard.context = flea::keys::Context::Listing;
            } else {
                producer.update(cx, |producer, cx| producer.key_action(action, cx));
            }
        }
        Some(true)
    }
}

#[path = "rail_compose.rs"]
mod compose;

#[cfg(test)]
#[path = "rail_tests.rs"]
mod u08;

#[cfg(test)]
#[path = "rail_devices_tests.rs"]
mod device_tests;

#[path = "../g2/pl1/rail_events.rs"]
mod device_events;
