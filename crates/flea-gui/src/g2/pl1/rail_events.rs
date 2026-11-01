//! Device replies enter the existing pane, status and unlock owners; a Quiesce runs before its release child starts.
use super::*;

impl Shell {
    pub(super) fn route_device_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let device_events = self
            .rail
            .as_ref()
            .map(|join| join.devices.read(cx).drain_events())
            .unwrap_or_default();
        for request in device_events {
            use crate::g2::pl1::devices::Effect;
            use crate::g2::pl1::source::Event;
            match &request.event {
                Event::Device(Effect::Opened(path)) => {
                    let opened = self.open_rail_place(request.side, path, window, cx);
                    self.land_device_open(request.side, opened, window, cx);
                }
                Event::Device(Effect::Message(note, error)) => {
                    // RailKeys.messaged: a failed mount has no open to land, so its focus claim goes with the error.
                    if *error {
                        self.drop_device_focus_claim();
                    }
                    let text = crate::g2::pl1::words::say(&self.locale.catalog, note);
                    self.status_messages(cx, |messages, now| {
                        messages.say(
                            &text,
                            *error,
                            "",
                            flea::ui_logic_fb1::status::Hint::None,
                            now,
                        )
                    });
                }
                Event::Device(Effect::Operation { headline, detail }) => {
                    self.drop_device_focus_claim();
                    let text = headline
                        .as_ref()
                        .map(|note| crate::g2::pl1::words::say(&self.locale.catalog, note))
                        .unwrap_or_default();
                    self.status_messages(cx, |messages, now| {
                        messages.say(
                            &text,
                            true,
                            detail,
                            flea::ui_logic_fb1::status::Hint::None,
                            now,
                        )
                    });
                }
                Event::Device(Effect::UnlockRequested {
                    device,
                    label,
                    size,
                    fstype,
                }) => {
                    let (device, label, fstype) = (device.clone(), label.clone(), fstype.clone());
                    self.open_unlock(request.side, &device, &label, *size, &fstype, window, cx);
                }
                Event::Device(Effect::Quiesce(volume)) => {
                    let volume = volume.clone();
                    self.quiesce_volume(request.side, &volume, window, cx);
                }
                _ => self.dispatch(Order::Device(request), window, cx),
            }
        }
    }

    pub(crate) fn open_rail_place(
        &mut self,
        side: usize,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let result = self.step(side, Step::OpenPlace(path.to_owned()), window, cx);
        self.panes
            .get(side)
            .and_then(Option::as_ref)
            .is_some_and(|host| {
                !host.stopped
                    && match result {
                        Ok(true) => true,
                        Ok(false) => host.nav.showing(&host.pane, path),
                        Err(_) => false,
                    }
            })
    }

    fn drop_device_focus_claim(&mut self) {
        if let Some(join) = &mut self.rail {
            if join.focus_on_open == Some(OpenClaim::Device) {
                join.focus_on_open = None;
            }
        }
    }

    /// RailKeys.landed: the open a keyboard Return asked for moves the keyboard to the list, unless it already left the rail.
    fn land_device_open(
        &mut self,
        side: usize,
        opened: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let claimed = self
            .rail
            .as_ref()
            .is_some_and(|join| join.focus_on_open == Some(OpenClaim::Device));
        self.drop_device_focus_claim();
        self.land_rail_open(claimed, side, opened, window, cx);
    }

    pub(crate) fn bind_rail_network_open(&mut self, origin: Option<u64>, generation: Option<u64>) {
        if let Some(join) = &mut self.rail {
            let bind = join.focus_on_open == Some(OpenClaim::NetworkPending)
                || matches!(join.focus_on_open, Some(OpenClaim::Network { origin: current, .. }) if Some(current) == origin);
            if bind {
                join.focus_on_open = origin.map(|origin| OpenClaim::Network { origin, generation });
            }
        }
    }

    pub(crate) fn drop_rail_network_open(&mut self, origin: u64) {
        if let Some(join) = &mut self.rail {
            if matches!(join.focus_on_open, Some(OpenClaim::Network { origin: current, .. }) if current == origin)
            {
                join.focus_on_open = None;
            }
        }
    }

    pub(crate) fn land_rail_network_open(
        &mut self,
        origin: u64,
        side: usize,
        opened: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let generation = self
            .rail
            .as_ref()
            .and_then(|join| join.network.as_ref())
            .map(|network| network.service.read(cx).mounts.generation);
        let claim = self.rail.as_ref().and_then(|join| join.focus_on_open);
        let claimed = match claim {
            Some(OpenClaim::Network {
                origin: current,
                generation: expected,
            }) if current == origin => {
                self.drop_rail_network_open(origin);
                expected.is_none_or(|expected| generation == Some(expected))
            }
            _ => false,
        };
        self.land_rail_open(claimed, side, opened, window, cx);
    }

    fn land_rail_open(
        &mut self,
        claimed: bool,
        side: usize,
        opened: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if claimed
            && opened
            && self.lifecycle.phase() == crate::window::lifecycle::Phase::Open
            && side == self.focus_side
            && self.dual_focus.rail.contains_focused(window, cx)
        {
            self.rail_focus(false, cx);
            window.focus(&self.focus, cx);
            self.keyboard.context = flea::keys::Context::Listing;
        }
    }
}
