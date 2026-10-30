//! The chooser's location press and grid wheel keep the existing entry and shared zoom policy.
use super::Picker;
use crate::grid::ZoomInput;
use crate::input::{scroll, wheel_delivery};
use gpui::{
    Context, MouseButton, MouseDownEvent, ScrollDelta, ScrollWheelEvent, TouchPhase, Window,
};

impl Picker {
    fn pointer_blocked(&self) -> bool {
        self.answered()
            || self.backend_unavailable()
            || self.accept_state().submitting()
            || self.chrome.folder_request.is_some()
            || self.chrome.menu.is_some()
            || !self.browse.route.asked.is_empty()
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

    pub(super) fn listing_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if self.pointer_blocked() || event.touch_phase == TouchPhase::Cancelled {
            return;
        }
        let view = self.browse.listing.read(cx);
        if view.view_mode() != "grid" || view.listing().is_pending() {
            return;
        }
        let state = view.grid_state();
        let state = state.borrow();
        if state.layout.is_none() || !state.bounds.contains(&event.position) {
            return;
        }
        drop(state);
        let zooming = event.modifiers.control && self.preferences.ctrl_zoom;
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
