//! Grid input uses the drawn tile mask and the shared key table, with no per-tile handlers.
use super::{arrow_with_wrap, host::GridHost, Arrow, Shown};
use crate::{
    list::pane::page_items,
    state::Shell,
    window::{
        input::{modifiers, reading},
        startup::View,
    },
};
use gpui::{Context, KeyDownEvent, MouseDownEvent, MouseMoveEvent, Window};

impl Shell {
    pub(crate) fn grid_sideways(
        &self,
        event: &KeyDownEvent,
        mirrored: bool,
    ) -> Option<&'static str> {
        if self.view != View::Grid
            || self.keyboard.context != flea::keys::Context::Listing
            || event.prefer_character_input
        {
            return None;
        }
        let mask = modifiers(event.keystroke.modifiers);
        match (
            mask,
            reading(event.keystroke.key.as_str(), mirrored),
            event
                .keystroke
                .key_char
                .as_deref()
                .map(|text| reading(text, mirrored)),
        ) {
            (0, "left", _) | (0, _, Some("h")) => Some("cursorLeft"),
            (0, "right", _) | (0, _, Some("l")) => Some("cursorRight"),
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
        let listing_total = host.pane.held.total;
        let shown = host
            .input
            .shown
            .as_deref()
            .map_or(Shown::All(listing_total), Shown::Only);
        let total = shown.total();
        let cursor = host.pane.cursor;
        // Close matches retain rank order; the input's inverse keeps arrows in drawn coordinates.
        let cursor_view = host
            .input
            .shown
            .view_of(cursor)
            .filter(|&view| view < total);
        let wrap_rows = flea::ui_logic_settings_pages::pages::is_true(
            &self.launch.settings.data,
            &["gridWrapRows"],
        );
        let wrap_ends = flea::ui_logic_settings_pages::pages::is_true(
            &self.launch.settings.data,
            &["wrapAtEnds"],
        );
        let layout = grid.view.layout(shown);
        let step = match action {
            "cursorUp" => Some(Arrow::Up),
            "cursorDown" => Some(Arrow::Down),
            "cursorLeft" | "extendLeft" => Some(Arrow::Left),
            "cursorRight" | "extendRight" => Some(Arrow::Right),
            "extendUp" => Some(Arrow::ExtendUp),
            "extendDown" => Some(Arrow::ExtendDown),
            _ => None,
        };
        let next = if let Some(step) = step {
            match (wrap_rows && wrap_ends, action, cursor_view, total) {
                // The filter's step: a hidden cursor stands before view 0, so any arrow lands on the first drawn row.
                (_, _, None, 1..) if host.input.shown.is_some() => Some(0),
                (true, "cursorLeft", Some(0), 1..) => Some(total - 1),
                (true, "cursorRight", Some(at), 1..) if at == total - 1 => Some(0),
                _ => arrow_with_wrap(layout.columns, cursor_view, total, step, wrap_rows),
            }
        } else {
            let page = page_items(host.pane.visible_rows());
            let at = cursor_view.unwrap_or(0);
            match action {
                "cursorFirst" => Some(0),
                "cursorLast" => total.checked_sub(1),
                "pageUp" => Some(at.saturating_sub(page)),
                "pageDown" => Some(at.saturating_add(page).min(total.saturating_sub(1))),
                _ => return false,
            }
        };
        if grid.next_view.is_some() {
            return true;
        }
        if let Some(grid) = host.grid.as_mut() {
            grid.state.borrow_mut().quiet = true;
            if grid.view.hovered.take().is_some() {
                cx.notify();
            }
            // Focus.js:166 cursorFirst sets the cursor even where it already is, so Home always brings the view back to its top.
            if action == "cursorFirst" && grid.view.visible && !grid.view.hidden_held {
                grid.view.glide = None;
                grid.view.scroll = 0.;
                cx.notify();
            }
        }
        if let Some(next) = next.and_then(|view| shown.at(view)) {
            if let Some(grid) = host.grid.as_mut() {
                grid.refollow();
            }
            if next != cursor {
                if matches!(
                    action,
                    "extendUp" | "extendDown" | "extendLeft" | "extendRight"
                ) {
                    let pulled = host.input.take_pull(cursor, host.pane.held.listing);
                    host.input.selection.extend(
                        &mut host.selected,
                        cursor,
                        next,
                        pulled,
                        &host.input.shown,
                    );
                    host.nav.landed = None;
                    host.pane.cursor = next;
                } else {
                    host.pane.cursor = next;
                    crate::input::follow_cursor(host, cursor);
                }
                self.seam.selected(next);
            }
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
        let pointer = Some((f32::from(event.position.x), f32::from(event.position.y)));
        // Only a real move lifts the keyboard's quiet; a synthetic move at the same point leaves the tile unlit.
        if state.quiet && grid.view.pointer == pointer {
            return;
        }
        grid.view.pointer = pointer;
        let painted = state
            .drawn()
            .iter()
            .find(|drawn| drawn.hovered)
            .map(|drawn| drawn.view);
        let quiet = state.quiet;
        drop(state);
        if quiet {
            grid.state.borrow_mut().quiet = false;
        }
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
        // Every press starts its own gesture, so no earlier tile press can lift rows for this one.
        host.input.press = None;
        // The glide lands in its own borrow, so a carry drop below can read the whole pane host.
        let Some(grid) = host.grid.as_mut() else {
            return;
        };
        grid.view.land_glide();
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
                .try_global::<crate::g2::file_clipboard::carry::Carrying>()
                .filter(|carrying| carrying.active())
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
                let dest = crate::g2::drag_drop::join::row_spot(host, row, &carried);
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
        if hit.is_none() {
            return;
        }
        drop(state);
        self.hold_pointer(side, true, window, cx);
        self.pointer_down(side, event, window, cx);
    }

    pub(crate) fn begin_grid_drag(&mut self, side: usize, window: &Window, cx: &mut Context<Self>) {
        // A travelled tile press cannot become a tap, even when its lift is refused.
        if let Some(press) = self.panes[side]
            .as_mut()
            .and_then(|host| host.input.press.as_mut())
        {
            press.dragged = true;
        }
        self.begin_file_drag(side, window, cx);
    }

    // Tests drive a scroll with no glide; production scrolls come through grid_scroll_at.
    #[cfg(test)]
    pub(crate) fn grid_scroll(&mut self, delta: f32, cx: &mut Context<Self>) -> bool {
        self.grid_scroll_at(delta, None, cx)
    }

    /// A scroll by `delta`, gliding from `glide_at` when a wheel notch asks for it.
    pub(crate) fn grid_scroll_at(
        &mut self,
        delta: f32,
        glide_at: Option<std::time::Instant>,
        cx: &mut Context<Self>,
    ) -> bool {
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
                let shown = host
                    .filter
                    .shown
                    .as_deref()
                    .map_or(Shown::All(host.pane.held.total), Shown::Only);
                grid.view
                    .wheel_by(shown, &GridHost::focus(&host.pane), delta, glide_at);
                cx.notify();
            }
        }
        true
    }
}
