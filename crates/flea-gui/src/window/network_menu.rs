//! The NETWORK row menu on mn1's shared card, the share browser over its own pane and the card's layer.
use super::Network;
use crate::{
    g2::mn1::{element::MenuElement, menu::Face, menu::Outcome},
    g2::pl2::{bookmarks, menus},
    list::Typeface,
    state::Shell,
    window::input::event_input,
};
use flea::keys::{self, Context as KeyContext, Frontend, Match};
use flea::ui_logic_mn1::fit::Rect;
use flea::ui_logic_pl4::rail;
use gpui::{
    div, prelude::*, px, AnyElement, Context, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, Window,
};

impl Shell {
    /// A NETWORK row's menu opens under the pointer, or under the row for a keyboard menu.
    pub(crate) fn network_menu(
        &mut self,
        side: usize,
        entry: &rail::RailEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure_network(window, cx);
        let Some(row) = self.network_row(entry) else {
            return;
        };
        let origin = self.network_origin(side);
        let Some(join) = &self.rail else {
            return;
        };
        let producer = join.places.read(cx);
        let laid = producer.laid.borrow();
        let pointer = producer.model.borrow().pointer_focus;
        let at = match producer.cursor().and_then(|index| laid.row(index)) {
            Some(drawn) if !pointer => (drawn.label_rect.x, drawn.rect.y + drawn.rect.height),
            _ => {
                let mouse = window.mouse_position();
                (f32::from(mouse.x), f32::from(mouse.y))
            }
        };
        drop(laid);
        let size = window.viewport_size();
        let typeface = Typeface::omarchy_default();
        let entries = menus::rows(&row);
        let env = self.menu_env(&typeface);
        let Some(network) = self.rail.as_ref().and_then(|join| join.network.as_ref()) else {
            return;
        };
        network.menu.borrow_mut().open(
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
        if let Some(network) = self.network() {
            network.menu_row = Some((row, origin));
        }
        cx.notify();
    }

    fn network_menu_outcome(
        &mut self,
        outcome: Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = match outcome {
            Outcome::Fire { action, .. } => action,
            Outcome::Closed | Outcome::Refused(_) => {
                if let Some(network) = self.network() {
                    network.menu_row = None;
                }
                cx.notify();
                return;
            }
            _ => {
                cx.notify();
                return;
            }
        };
        let Some((row, origin)) = self.network().and_then(|network| network.menu_row.take()) else {
            return;
        };
        let side = self.network_side(origin).unwrap_or(self.focus_side);
        let entry = section_entry(&row);
        match action.as_str() {
            "open" => self.network_activate(side, &entry, window, cx),
            "unmount" => self.network_unmount(&row, origin, cx),
            "remove" => self.network_save(bookmarks::Edit::Remove { uri: row.uri }, None, cx),
            "editPlace" => {
                if let Some(network) = self.network() {
                    let card = network.card.clone();
                    card.update(cx, |card, cx| {
                        card.location(&row.uri, &row.label, true, origin, window, cx)
                    });
                }
            }
            "rename" => {
                if let Some(join) = &self.rail {
                    let producer = join.places.clone();
                    let index = producer.read(cx).cursor();
                    if let Some(index) = index {
                        producer.update(cx, |producer, cx| producer.start_rename(index, cx));
                        self.rail_focus(true, cx);
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    /// The menu and the share browser take their keys before the rail and the listing; the card keeps its own focus.
    pub(crate) fn network_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(network) = self.rail.as_ref().and_then(|join| join.network.as_ref()) else {
            return false;
        };
        let menu_open = network.menu.borrow().opened();
        let shares = network.shares.clone();
        if !menu_open && !shares.read(cx).model.open {
            return false;
        }
        let preset = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default");
        let context = if menu_open {
            KeyContext::Menu
        } else {
            KeyContext::Listing
        };
        let Match::Action(action) = keys::lookup(
            preset,
            context,
            Frontend::Gui,
            event_input(event, window.modifiers()),
        ) else {
            return true;
        };
        if menu_open {
            let typeface = Typeface::omarchy_default();
            let env = self.menu_env(&typeface);
            let menu = network.menu.clone();
            let outcome = menu.borrow_mut().act(action, None, "", &env, window);
            self.network_menu_outcome(outcome, window, cx);
        } else {
            shares.update(cx, |shares, cx| shares.action(action, cx));
        }
        true
    }

    /// The card covers the window, the browser covers its own pane, and the row menu sits over both.
    pub(crate) fn network_layers(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(network) = self.rail.as_ref().and_then(|join| join.network.as_ref()) else {
            return Vec::new();
        };
        let mut layers = Vec::new();
        let browser = network.shares.read(cx);
        if browser.model.open {
            if let Some(side) = self.network_side(browser.model.origin) {
                let rect = self.geometry(window).panes[side];
                layers.push(
                    div()
                        .absolute()
                        .left(px(rect.x))
                        .top(px(rect.y))
                        .w(px(rect.width))
                        .h(px(rect.height))
                        .child(network.shares.clone())
                        .into_any_element(),
                );
            }
        }
        if network.card.read(cx).model.opened {
            layers.push(
                gpui::deferred(div().absolute().inset_0().child(network.card.clone()))
                    .with_priority(2)
                    .into_any_element(),
            );
        }
        if let Some(menu) = self.network_menu_layer(network, cx) {
            layers.push(menu);
        }
        layers
    }

    fn network_menu_layer(&self, network: &Network, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !network.menu.borrow().opened() {
            return None;
        }
        Some(
            gpui::deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .on_mouse_move(cx.listener(|shell, event: &MouseMoveEvent, window, cx| {
                        let typeface = Typeface::omarchy_default();
                        let env = shell.menu_env(&typeface);
                        if let Some(network) = shell.rail.as_ref().and_then(|j| j.network.as_ref())
                        {
                            network.menu.borrow_mut().hover(
                                event.position.x.into(),
                                event.position.y.into(),
                                &env,
                                window,
                            );
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::network_menu_click))
                    .on_mouse_down(MouseButton::Right, cx.listener(Self::network_menu_click))
                    .child(MenuElement::new(network.menu.clone(), px(self.corner))),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }

    fn network_menu_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self
            .rail
            .as_ref()
            .and_then(|join| join.network.as_ref())
            .map(|network| network.menu.clone())
        else {
            return;
        };
        let typeface = Typeface::omarchy_default();
        let env = self.menu_env(&typeface);
        let outcome = menu.borrow_mut().click(
            event.position.x.into(),
            event.position.y.into(),
            None,
            "",
            &env,
            window,
        );
        self.network_menu_outcome(outcome, window, cx);
        cx.stop_propagation();
    }
}

/// The rail-shaped key a join row is found by again.
fn section_entry(row: &flea::ui_logic_pl1::RailEntry) -> rail::RailEntry {
    rail::RailEntry {
        path: row.path.clone(),
        uri: row.uri.clone(),
        ..rail::RailEntry::default()
    }
}
