//! The chooser's location press, grid wheel and row clicks keep the existing entry, zoom policy and activation contract.
use super::Picker;
use crate::grid::ZoomInput;
use crate::input::{scroll, wheel_delivery};
use flea::ui_logic_picker_portal::{self as logic, DoubleAction};
use gpui::{
    Context, MouseButton, MouseDownEvent, ScrollDelta, ScrollWheelEvent, TouchPhase, Window,
};

impl Picker {
    fn pointer_blocked(&self) -> bool {
        self.answered()
            || self.backend_unavailable()
            || self.accept_state().submitting()
            || self.folder_busy()
            || self.chrome.menu.is_some()
            || !self.browse.route.asked.is_empty()
    }

    // Single-click mode suppresses repeat taps on one row but activates a different row; double-click mode requires the same target twice.
    fn row_activates(&self, clicks: usize, first_path: &str, path: &str) -> bool {
        if self.preferences.followed.single_click {
            clicks == 1 || !logic::same_tap(first_path, path)
        } else {
            clicks == 2 && logic::same_tap(first_path, path)
        }
    }

    pub(super) fn row_pointer(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        if self.backend_unavailable()
            || self.answered()
            || self.accept_state().submitting()
            || self.listing.failed
            || self.listing.state != "ready"
            || self.chrome.menu_view.is_some()
        {
            return;
        }
        if self.header_press(event, cx) {
            return;
        }
        let hit = self
            .browse
            .listing
            .update(cx, |view, _| view.pointer_hit(event.position));
        let Some(hit) = hit else { return };
        if event.modifiers.shift && self.marks_allowed() {
            self.extend_to(hit.index, cx);
            return;
        }
        self.end_range();
        self.move_cursor(hit.index, cx);
        if hit.checkbox {
            self.toggle_mark(hit.index, cx);
        } else if logic::saving(&self.req) && !hit.directory {
            self.activate_row(hit.index, cx);
            // A row click names Filename but keeps the keyboard on the listing, as v0.3.12 does.
            self.browse.focus_filename = false;
        } else if self.row_activates(event.click_count, &hit.first_path, &hit.path) {
            match self.double_activate(Some(hit.directory), &hit.path, &hit.path, cx) {
                DoubleAction::Open => self.open_folder(&hit.path, cx),
                DoubleAction::Mark => self.toggle_mark(hit.index, cx),
                _ => {}
            }
        }
    }

    pub(super) fn path_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        // The open filter menu's window-wide card sits above and takes this press as its outside click.
        if self.chrome.menu_view.is_some() {
            return;
        }
        if self.pointer_blocked() {
            cx.stop_propagation();
            return;
        }
        let path = self.browse.path_view(cx);
        if path.read(cx).editing() {
            return;
        }
        self.browse.window = Some(window.window_handle());
        path.update(cx, |path, cx| path.open(window, cx));
        cx.stop_propagation();
    }

    // The mouse side buttons follow the main window's pointer rows in keys.toml: Back goes back or up, Forward retraces a Back.
    pub(super) fn side_button(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !matches!(event.button, MouseButton::Navigate(_))
            || self.pointer_blocked()
            || self.accept_state().collision_review()
            || self.browse.path_view(cx).read(cx).holds_keyboard(window)
        {
            return;
        }
        let Some(binding) = crate::window::input::pointer("window", "any", event) else {
            return;
        };
        match binding.effect {
            "backOrParent" if self.chrome.history.is_empty() => self.go_up(cx),
            "backOrParent" => self.go_back(cx),
            "forward" => self.go_forward(cx),
            _ => return,
        }
        // The press is the window's, so a row under it never takes it for a click.
        cx.stop_propagation();
    }

    pub(super) fn listing_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if self.pointer_blocked() || event.touch_phase == TouchPhase::Cancelled {
            return;
        }
        let view = self.browse.listing.read(cx);
        if view.listing().is_pending() {
            return;
        }
        if view.view_mode() == "list" {
            return self.list_wheel(event, cx);
        }
        let state = view.grid_state();
        let state = state.borrow();
        if state.layout.is_none() || !state.bounds.contains(&event.position) {
            return;
        }
        drop(state);
        let zooming = event.modifiers.control && self.preferences.followed.ctrl_zoom;
        let Some(delivery) = wheel_delivery(event, true) else {
            return;
        };
        if delivery.pixel == 0. && delivery.angle == 0. {
            return;
        }
        let input = ZoomInput {
            touchpad: scroll::is_touchpad(delivery.phase),
            pixels: delivery.pixel as f32,
            angle: delivery.angle as f32,
        };
        let next = self.browse.listing.update(cx, |view, cx| {
            let next = if zooming {
                view.zoom_grid(input)
            } else {
                let delta = match event.delta {
                    ScrollDelta::Pixels(_) => scroll::touch_distance(delivery.pixel) as f32,
                    _ => f32::from(scroll::wheel_delta(&event.delta).y),
                };
                let grid = view.grid_view();
                let shown = view.shown().map_or(
                    crate::grid::Shown::All(view.listing().total()),
                    crate::grid::Shown::Only,
                );
                view.set_scroll_y(grid.layout(shown).clamp(view.scroll_y() - delta));
                None
            };
            if !zooming || next.is_some() {
                cx.notify();
            }
            next
        });
        if let Some(size) = next {
            self.persist_thumbnail(size, cx);
        }
        if !zooming || next.is_some() {
            self.refill_grid(cx);
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn list_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let bounds = self.browse.listing.read(cx).drawn().drawn.bounds;
        let Some(delivery) = wheel_delivery(event, true) else {
            return;
        };
        if !bounds.contains(&event.position) || (delivery.pixel == 0. && delivery.angle == 0.) {
            return;
        }
        let delta = match event.delta {
            ScrollDelta::Pixels(_) => scroll::touch_distance(delivery.pixel) as f32,
            _ => f32::from(scroll::wheel_delta(&event.delta).y),
        };
        let ask = self.browse.listing.update(cx, |view, cx| {
            view.list_scroll_by(delta);
            cx.notify();
            view.list_window_request()
        });
        if let Some(line) = ask {
            self.send_listing(&line);
        }
        cx.notify();
        cx.stop_propagation();
    }

    pub(super) fn refill_grid(&self, cx: &mut Context<Self>) {
        // Rows are data, not input: a menu or pending request never holds back the rows a grid draws.
        if self.answered() || self.backend_unavailable() {
            return;
        }
        if let Some(line) = self.browse.listing.read(cx).grid_window_request() {
            self.send_listing(&line);
        }
    }
}
