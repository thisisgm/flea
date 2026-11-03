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
        self.grid_wheel(event, window, cx);
    }
}
