//! Only the painted Grid viewport accepts root wheel events.
use crate::{state::Shell, window::startup};
use gpui::{Context, ScrollWheelEvent, Window};

impl Shell {
    pub(crate) fn wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_wheel(event, window, cx) {
            return;
        }
        if self.input_blocked(cx) || self.view != startup::View::Grid {
            return;
        }
        let Some(host) = self.panes[self.focus_side].as_ref() else {
            return;
        };
        // The root hears chrome and status wheels too; only the painted Grid owns scrolling.
        if host.drawn_view != Some(startup::View::Grid)
            || host.grid.as_ref().is_none_or(|grid| {
                let drawn = grid.state.borrow();
                drawn.layout.is_none() || !drawn.bounds.contains(&event.position)
            })
        {
            return;
        }
        let delta = -f32::from(crate::input::scroll::wheel_delta(&event.delta).y);
        // A notch glides as List's does; a touchpad delta, or reduced motion, lands at once.
        let glide_at = (matches!(event.delta, gpui::ScrollDelta::Lines(_)) && !cx.reduce_motion())
            .then(|| cx.background_executor().now());
        self.grid_scroll_at(delta, glide_at, cx);
    }
}
