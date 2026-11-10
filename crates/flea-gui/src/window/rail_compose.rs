//! Compose the accepted U08 producer with this window's existing state and focus owners.
use super::*;

/// Builds the producer from Settings.data and the host over it; the host is the only lazy owner.
pub fn mount(catalog: Arc<I18n>, state: &Json, side: usize, cx: &mut Context<Shell>) -> Join {
    #[cfg(not(test))]
    let (runner, seed) = (
        Rc::new(crate::g2::device_rail::process::ThreadRunner),
        crate::g2::device_rail::source::seed::take(),
    );
    #[cfg(not(test))]
    let hostname = seed.as_ref().map(|seed| seed.hostname.clone());
    #[cfg(test)]
    let ((runner, hostname), seed) = (crate::g2::device_rail::source::fixture::services(cx), None);
    mount_with_source(catalog, state, side, runner, hostname, seed, cx)
}

pub(crate) fn mount_with_source(
    catalog: Arc<I18n>,
    state: &Json,
    side: usize,
    runner: Rc<dyn Runner>,
    hostname: Option<String>,
    seed: Option<crate::g2::device_rail::source::seed::Seed>,
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
    let devices = cx.new(|_| {
        let mut source = Source::new(runner, hostname, state);
        // Only a rail drawn at launch takes it: a hidden one lists fresh when it is revealed.
        if let Some(seed) = seed.filter(|_| mode == Mode::Docked) {
            source.seed(seed.listing);
        }
        source
    });
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
        |shell, _, event: &crate::g2::trash_browser::join::TrashEvent, cx| match event {
            crate::g2::trash_browser::join::TrashEvent::Count(count) => {
                shell.rail_trash_count(*count as u64, cx);
            }
            crate::g2::trash_browser::join::TrashEvent::FocusRail => {
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
        focus_on_open: None,
        _trash: trash_subscription,
        network: None,
        network_runner: crate::window::network::default_runner(),
        unlock: None,
        device_runner,
        add_seen: 0,
    }
}

impl Shell {
    pub(crate) fn rail_locale(&mut self, catalog: &Arc<I18n>, cx: &mut Context<Self>) {
        if let Some(join) = &self.rail {
            join.places.update(cx, |producer, cx| {
                producer.set_locale(Arc::clone(catalog), cx);
            });
        }
    }

    /// The same mount is called by production construction and the mounted regression fixture.
    pub(crate) fn mount_rail(&mut self, cx: &mut Context<Self>) {
        if self.rail.is_some() {
            return;
        }
        let join = mount(
            Arc::clone(&self.locale.catalog),
            &self.launch.settings.data,
            self.focus_side,
            cx,
        );
        join.places.read(cx).model.borrow_mut().cover = self.settings.cover.clone();
        self.slots.rail = Some(AnyView::from(join.host.clone()));
        self.rail = Some(join);
        self.receive_rail_drops(cx);
    }

    pub(crate) fn sync_rail(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(join) = &self.rail else {
            return;
        };
        if join.host.read(cx).rail.mode() == Mode::Hidden {
            self.input.remove("rail");
            if self.dual_focus.rail.contains_focused(window, cx) {
                window.focus(&self.overlay_return_focus(cx), cx);
                self.keyboard.context = flea::keys::Context::Listing;
            }
            return;
        }
        self.input.register(crate::input::focus::Stop {
            name: "rail",
            handle: self.dual_focus.rail.clone(),
            context: flea::keys::Context::Rail,
        });
        let held = self.dual_focus.rail.contains_focused(window, cx);
        let changed = join.host.read(cx).rail.wants().focus != held;
        if changed {
            join.host.update(cx, |host, cx| host.focus(held, cx));
        }
        if !join.host.read(cx).rail.shown() {
            return;
        }
        let face = self.active_typeface();
        let menu_open = join
            .network
            .as_ref()
            .is_some_and(|network| network.menu.borrow().opened());
        join.places.update(cx, |producer, cx| {
            producer.side = self.focus_side;
            let mut model = producer.model.borrow_mut();
            let changed =
                model.focused != held || model.menu_open != menu_open || producer.typeface != face;
            model.focused = held;
            model.menu_open = menu_open;
            model.corner = self.corner;
            drop(model);
            if !held {
                producer.cancel_rename(cx);
            }
            if changed {
                producer.typeface = face;
                cx.notify();
            }
        });
    }
}

#[cfg(test)]
#[path = "rail_compose_tests.rs"]
mod tests;
