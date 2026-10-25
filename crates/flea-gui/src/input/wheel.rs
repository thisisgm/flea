//! Wheel delivery: one notch is three lines of 24px at the pinned platform, and the cursor follows the view.
use crate::state::Shell;
use gpui::{Context, ScrollDelta, ScrollWheelEvent, Window};

impl Shell {
    pub(crate) fn pointer_wheel(
        &mut self,
        side: usize,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_blocked(cx) {
            return;
        }
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        if host.pane.busy() || host.input.band.is_some() {
            return;
        }
        let Some(frame) = host.input.frame else {
            return;
        };
        let delta = match event.delta {
            ScrollDelta::Pixels(p) => -f32::from(p.y) * 2.5,
            ScrollDelta::Lines(p) => {
                -p.y * (super::WHEEL_LINES as f32 / super::PLATFORM_SCROLL_GAIN) * 24. * 4.
            }
        };
        if event.modifiers.control {
            return;
        }
        let before = host.pane.scroll_y;
        host.pane
            .scroll_to((before + delta).clamp(0., frame.max_scroll()));
        host.input.keyboard();
        if before != host.pane.scroll_y {
            host.input
                .scrolled(host.pane.scroll_y, &mut host.pane.cursor);
            host.input.moved(cx.background_executor().now());
            cx.stop_propagation();
        }
        self.arm_input_tick(side, window, cx);
        cx.notify();
    }
}
