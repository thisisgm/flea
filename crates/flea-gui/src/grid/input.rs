//! Grid input uses the drawn tile mask and the shared key table, with no per-tile handlers.
use super::{arrow, host::GridHost, Arrow, Shown};
use crate::{
    state::Shell,
    window::{
        dispatch::Order,
        input::{modifiers, pointer, ActionEvent},
        startup::View,
    },
};
use gpui::{Context, KeyDownEvent, MouseDownEvent, MouseMoveEvent, Window};

impl Shell {
    pub(crate) fn grid_sideways(&self, event: &KeyDownEvent) -> Option<&'static str> {
        if self.view != View::Grid
            || self.keyboard.context != flea::keys::Context::Listing
            || modifiers(event.keystroke.modifiers) != 0
            || event.prefer_character_input
        {
            return None;
        }
        match (
            event.keystroke.key.as_str(),
            event.keystroke.key_char.as_deref(),
        ) {
            ("left", _) | (_, Some("h")) => Some("cursorLeft"),
            ("right", _) | (_, Some("l")) => Some("cursorRight"),
            _ => None,
        }
    }

    pub(crate) fn grid_key(&mut self, action: &str, cx: &mut Context<Self>) -> bool {
        if self.view != View::Grid || self.keyboard.context != flea::keys::Context::Listing {
            return false;
        }
        let Some(host) = self.panes[self.focus_side].as_mut() else {
            return false;
        };
        let Some(grid) = host.grid.as_ref() else {
            return false;
        };
        let total = host.pane.held.total;
        let cursor = host.pane.cursor;
        let layout = grid.view.layout(Shown::All(total));
        let step = match action {
            "cursorUp" => Some(Arrow::Up),
            "cursorDown" => Some(Arrow::Down),
            "cursorLeft" => Some(Arrow::Left),
            "cursorRight" => Some(Arrow::Right),
            "extendUp" => Some(Arrow::ExtendUp),
            "extendDown" => Some(Arrow::ExtendDown),
            _ => None,
        };
        let next = if let Some(step) = step {
            arrow(layout.columns, Some(cursor), total, step)
        } else {
            let page = layout.visible_rows() * layout.columns;
            match action {
                "cursorFirst" => Some(0),
                "cursorLast" => total.checked_sub(1),
                "pageUp" => Some(cursor.saturating_sub(page)),
                "pageDown" => Some(cursor.saturating_add(page).min(total.saturating_sub(1))),
                _ => return false,
            }
        };
        if grid.next_view.is_some() {
            return true;
        }
        if let Some(next) = next.filter(|next| *next < total && *next != cursor) {
            if matches!(action, "extendUp" | "extendDown") {
                host.selected.extend(cursor.min(next)..=cursor.max(next));
                host.selected.sort_unstable();
                host.selected.dedup();
            }
            host.pane.cursor = next;
            self.seam.selected(next);
            cx.notify();
        }
        true
    }

    pub(crate) fn grid_hover(
        &mut self,
        side: usize,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if self.view != View::Grid {
            return;
        }
        let Some(grid) = self.panes[side]
            .as_mut()
            .and_then(|host| host.grid.as_mut())
        else {
            return;
        };
        let state = grid.state.borrow();
        let at = event.position - state.bounds.origin;
        let hit = state.tile_at(f32::from(at.x), f32::from(at.y));
        grid.view.pointer = Some((f32::from(event.position.x), f32::from(event.position.y)));
        let painted = state
            .drawn()
            .iter()
            .find(|drawn| drawn.hovered)
            .map(|drawn| drawn.view);
        if grid.view.hovered != hit || painted != hit {
            grid.view.hovered = hit;
            cx.notify();
        }
    }

    pub(crate) fn grid_leave(&mut self, side: usize, cx: &mut Context<Self>) {
        if let Some(grid) = self.panes[side]
            .as_mut()
            .and_then(|host| host.grid.as_mut())
        {
            let pointer = grid.view.pointer.take().is_some();
            let hovered = grid.view.hovered.take().is_some();
            if pointer || hovered {
                cx.notify();
            }
        }
    }

    pub(crate) fn grid_press(
        &mut self,
        side: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view != View::Grid || self.input_blocked(cx) {
            return;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if host.pane.busy() || host.stopped {
            return;
        }
        let Some(grid) = host.grid.as_ref() else {
            return;
        };
        if grid.next_view.is_some() {
            return;
        }
        let state = grid.state.borrow();
        if state
            .frame_held
            .as_ref()
            .is_none_or(|held| !std::rc::Rc::ptr_eq(held, &host.pane.held))
        {
            return;
        }
        let at = event.position - state.bounds.origin;
        let hit = state.tile_at(f32::from(at.x), f32::from(at.y));
        if event.button == gpui::MouseButton::Left && state.bounds.contains(&event.position) {
            if let Some(carrying) = cx
                .try_global::<crate::g2::dg1::carry::Carrying>()
                .filter(|carrying| carrying.carry.active())
            {
                // Owned copy first: the cx borrow must end before stop_propagation.
                let carried = carrying.source_rows(window, side, host).to_vec();
                cx.stop_propagation();
                host.input.press = None;
                if host.nav.gate.owned() {
                    return;
                }
                let row = hit
                    .and_then(|hit| state.drawn().iter().find(|drawn| drawn.view == hit))
                    .map(|drawn| drawn.listing);
                if hit.is_some() && row.is_none_or(|row| host.pane.held.row(row).is_none()) {
                    return;
                }
                let dest = crate::g2::dg2::join::row_spot(host, row, &carried);
                drop(state);
                self.carry_land(
                    side,
                    &dest,
                    event.modifiers.control,
                    event.modifiers.shift,
                    window,
                    cx,
                );
                return;
            }
        }
        let Some(hit) = hit else { return };
        let Some(drawn) = state.drawn().iter().find(|drawn| drawn.view == hit) else {
            return;
        };
        let row = drawn.listing;
        let Some(facts) = host.pane.held.row(row) else {
            return;
        };
        let Some(binding) = pointer("listing", if facts.dir { "dir" } else { "file" }, event)
        else {
            return;
        };
        drop(state);
        let old = host.pane.cursor;
        match binding.effect {
            "selectOnly" => {
                host.selected = vec![row];
                host.pane.cursor = row;
            }
            "toggleSelect" if event.click_count == 1 => {
                if host.selected.binary_search(&row).is_ok() {
                    host.selected.retain(|at| *at != row);
                } else {
                    host.selected.push(row);
                    host.selected.sort_unstable();
                }
                host.pane.cursor = row;
            }
            "extendSelect" if event.click_count == 1 => {
                host.selected = (old.min(row)..=old.max(row)).collect();
                host.pane.cursor = row;
            }
            "open" | "menu" => {
                host.pane.cursor = row;
                if host.selected.binary_search(&row).is_err() {
                    host.selected = vec![row];
                }
            }
            _ => {}
        }
        self.focus_side = side;
        if matches!(binding.effect, "open" | "menu" | "openTab") {
            let event = ActionEvent {
                action: binding.effect,
                context: flea::keys::Context::Listing,
                seat: flea::keys::sequence::Seat {
                    listing: host.pane.held.listing as u64,
                    cursor: row,
                    view: self.view as u64,
                    focus: side as u64,
                    ..Default::default()
                },
                pair_armed: false,
            };
            self.dispatch(Order::Action(event), window, cx);
        }
        cx.notify();
    }

    pub(crate) fn grid_scroll(&mut self, delta: f32, cx: &mut Context<Self>) -> bool {
        if self.view != View::Grid {
            return false;
        }
        if let Some(host) = self.panes[self.focus_side]
            .as_mut()
            .filter(|host| !host.pane.busy())
        {
            if let Some(grid) = host.grid.as_mut() {
                if grid.next_view.is_some() {
                    return true;
                }
                grid.view.scroll_by(
                    Shown::All(host.pane.held.total),
                    &GridHost::focus(&host.pane),
                    delta,
                );
                cx.notify();
            }
        }
        true
    }
}
