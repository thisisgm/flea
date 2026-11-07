//! The NETWORK plus with List nearby hosts ticked: a menu of the hosts the pane's backend finds, and Add a Network Share… last.
use super::Network;
use crate::{
    backend::Event,
    g2::mn1::menu::{Face, Outcome},
    g2::pl2::nearby::{self, Host, State},
    state::Shell,
};
use flea::jsondoc::Json;
use flea::ui_logic_mn1::fit::Rect;
use flea::ui_logic_pl4::rail::Group;
use gpui::{Context, Window};

/// One open hosts menu: the pane that asked, its request id and the answer the rows were drawn from.
pub(crate) struct Nearby {
    pub(crate) side: usize,
    pub(crate) id: usize,
    pub(crate) state: State,
    pub(crate) hosts: Vec<Host>,
    /// A key, a click or a pointer move over the rows since the menu opened; the cursor then stays where the person put it.
    pub(crate) touched: bool,
    _quiet: gpui::Task<()>,
}

/// Past the backend's three-second scan, with room for its helpers to be reaped.
const QUIET: std::time::Duration = std::time::Duration::from_secs(5);

/// The option ships off, and it rides the NETWORK group, so an unticked Network turns it off too.
pub(crate) fn enabled(state: &Json) -> bool {
    super::shown(state)
        && state
            .get("places")
            .and_then(|places| places.get("nearbyHosts"))
            .and_then(Json::as_bool)
            == Some(true)
}

impl Shell {
    /// The heading's plus: with the option off it opens Add a Network Share exactly as 0.4.0 does; on, it lists hosts first.
    pub(crate) fn network_plus(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !enabled(&self.launch.settings.data) {
            self.network_add(side, window, cx);
            return;
        }
        self.ensure_network(window, cx);
        self.nearby_stop();
        // A rail with nothing laid out yet has no group to sit under, so the menu opens at the pointer as a row menu does.
        let at = self.nearby_anchor(cx).unwrap_or_else(|| {
            let mouse = window.mouse_position();
            (f32::from(mouse.x), f32::from(mouse.y))
        });
        let Some(network) = self.network() else {
            return;
        };
        network.nearby_serial += 1;
        let id = network.nearby_serial;
        let line = nearby::request("start", id);
        let asked = self.panes[side]
            .as_ref()
            .and_then(|host| host.backend.as_ref())
            .is_some_and(|backend| backend.line(line).is_ok());
        let state = if asked { State::Looking } else { State::Empty };
        let size = window.viewport_size();
        let typeface = self.active_typeface();
        let env = self.menu_env(&typeface);
        let entries = nearby::entries(state, &[], &self.locale.catalog);
        let Some(network) = self.rail.as_ref().and_then(|join| join.network.as_ref()) else {
            return;
        };
        let mut menu = network.menu.borrow_mut();
        menu.minimum = nearby::MENU_MIN;
        menu.open(
            Face::Place,
            entries,
            at,
            Rect {
                x: 0.,
                y: 0.,
                w: size.width.into(),
                h: size.height.into(),
            },
            "",
            &env,
            window,
        );
        drop(menu);
        // The backend answers within its own three seconds; a pane whose backend went away must not leave the busy row up.
        let quiet = cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(QUIET).await;
            let _ = this.update_in(cx, |shell, window, cx| shell.nearby_quiet(id, window, cx));
        });
        if let Some(network) = self.network() {
            network.menu_row = None;
            network.nearby = Some(Nearby {
                side,
                id,
                state,
                hosts: Vec::new(),
                touched: false,
                _quiet: quiet,
            });
        }
        cx.notify();
    }

    /// A scan with no answer past the backend's own limit reads as nothing found.
    fn nearby_quiet(&mut self, id: usize, window: &mut Window, cx: &mut Context<Self>) {
        let catalog = self.locale.catalog.clone();
        let Some(network) = self.network() else {
            return;
        };
        let Network { nearby, menu, .. } = network;
        let Some(open) = nearby
            .as_mut()
            .filter(|open| open.id == id && open.state == State::Looking)
        else {
            return;
        };
        open.state = State::Empty;
        if menu.borrow().opened() {
            let menu = menu.clone();
            self.nearby_refresh(
                &menu,
                nearby::entries(State::Empty, &[], &catalog),
                false,
                window,
                cx,
            );
        }
    }

    /// Under the NETWORK group's last row at the rail's mark column, so the group stays in view; under the heading when it is empty.
    fn nearby_anchor(&self, cx: &Context<Self>) -> Option<(f32, f32)> {
        let producer = self.rail.as_ref()?.places.read(cx);
        let laid = producer.laid.borrow();
        let mark_x = crate::list::Metrics::for_typeface(&producer.typeface).pad;
        let last = producer
            .entries()
            .iter()
            .rposition(|entry| entry.group == Group::Network)
            .and_then(|index| laid.row(index));
        if let Some(row) = last {
            return Some((row.rect.x + mark_x, row.rect.y + row.rect.height));
        }
        let header = laid.headers.iter().find(|header| header.add.is_some())?;
        Some((header.rect.x + mark_x, header.rect.y + header.rect.height))
    }

    /// The backend's answer refreshes the open menu; an answer for a menu that closed or moved on lands nowhere.
    pub(crate) fn nearby_event(
        &mut self,
        side: usize,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Event::Producer(line) = event else {
            return false;
        };
        let Some(reply) = nearby::parse(line) else {
            return false;
        };
        let catalog = self.locale.catalog.clone();
        let Some(network) = self.network() else {
            return true;
        };
        let Network { nearby, menu, .. } = network;
        let Some(open) = nearby
            .as_mut()
            .filter(|open| open.side == side && open.id == reply.id)
        else {
            return true;
        };
        if reply.state == State::Looking || !menu.borrow().opened() {
            return true;
        }
        open.state = reply.state;
        open.hosts = reply.hosts;
        let entries = nearby::entries(open.state, &open.hosts, &catalog);
        let (menu, untouched) = (menu.clone(), !open.touched);
        self.nearby_refresh(&menu, entries, untouched, window, cx);
        true
    }

    fn nearby_refresh(
        &self,
        menu: &std::rc::Rc<std::cell::RefCell<crate::g2::mn1::menu::Menu>>,
        entries: Vec<flea::ui_logic_mn1::Entry>,
        untouched: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let typeface = self.active_typeface();
        let env = self.menu_env(&typeface);
        let mut menu = menu.borrow_mut();
        // The first host takes the cursor from Add a Network Share… only if the person has not acted since the menu opened.
        let resting = untouched
            && menu.card.as_ref().is_some_and(|card| {
                card.cursor
                    .is_some_and(|at| card.entries[at].action == nearby::ADD_SHARE)
            });
        let first_host = entries.first().is_some_and(|entry| entry.steppable());
        menu.refresh(entries, &env, window);
        if resting && first_host {
            menu.land(0);
        }
        cx.notify();
    }

    /// The person acted on the open hosts menu, so an answer that lands later leaves the cursor alone.
    pub(crate) fn nearby_touch(&mut self) {
        if let Some(open) = self.network().and_then(|network| network.nearby.as_mut()) {
            open.touched = true;
        }
    }

    /// Ends the scan the hosts menu asked for, if one is still out; the menu itself is closed by its caller.
    pub(crate) fn nearby_stop(&mut self) {
        let Some(open) = self.network().and_then(|network| network.nearby.take()) else {
            return;
        };
        if open.state == State::Looking {
            if let Some(backend) = self.panes[open.side]
                .as_ref()
                .and_then(|host| host.backend.as_ref())
            {
                let _ = backend.line(nearby::request("stop", open.id));
            }
        }
    }

    /// A host fills the card and nothing connects; Add a Network Share… opens the card at rest.
    pub(super) fn nearby_outcome(
        &mut self,
        outcome: Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Outcome::Fire { action, .. } = outcome else {
            if matches!(outcome, Outcome::Closed | Outcome::Refused(_)) {
                self.nearby_stop();
            }
            cx.notify();
            return;
        };
        let Some(open) = self.network().and_then(|network| network.nearby.as_ref()) else {
            return;
        };
        let side = open.side;
        let host = nearby::chosen(&action, &open.hosts).cloned();
        self.nearby_stop();
        if let Some(network) = self.network() {
            network.menu.borrow_mut().close();
        }
        match host {
            Some(host) => {
                let origin = self.network_origin(side);
                self.network_remember_focus();
                if let Some(network) = self.network() {
                    let card = network.card.clone();
                    card.update(cx, |card, cx| {
                        card.open_suggested(origin, &host.address, window, cx)
                    });
                }
            }
            None if action == nearby::ADD_SHARE => self.network_add(side, window, cx),
            None => {}
        }
        cx.notify();
    }
}
