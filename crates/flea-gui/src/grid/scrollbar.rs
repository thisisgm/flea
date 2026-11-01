//! The shared viewport scroller drives Grid's own position without changing its cursor or selection.
use super::{host::GridHost, Shown};
use crate::{
    input::{scroll::Bar, Frame},
    state::Shell,
    window::pane::PaneHost,
};
use gpui::{Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window};

fn scroll_to(host: &mut PaneHost, frame: Frame, position: f32, now: std::time::Instant) {
    let Some(grid) = host.grid.as_mut() else {
        return;
    };
    let shown = host
        .filter
        .shown
        .as_deref()
        .map_or(Shown::All(host.pane.held.total), Shown::Only);
    grid.view.land_glide();
    grid.view.scroll_by(
        shown,
        &GridHost::focus(&host.pane),
        position - grid.view.scroll,
    );
    if grid.view.scroll != frame.scroll {
        host.input.moved(now);
    }
}

impl Shell {
    pub(crate) fn grid_bar_down(
        &mut self,
        side: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_blocked(cx) || event.button != MouseButton::Left {
            return;
        }
        let Some(host) = self.panes[side]
            .as_mut()
            .filter(|host| !host.pane.busy() && !host.stopped)
        else {
            return;
        };
        let Some(frame) = host
            .input
            .frame
            .filter(|frame| frame.listing == host.pane.held.listing)
        else {
            return;
        };
        if !frame.in_lane(event.position)
            || frame.max_scroll() <= 0.5
            || host
                .grid
                .as_ref()
                .is_none_or(|grid| grid.next_view.is_some())
        {
            return;
        }
        let bar = Bar::new(frame);
        let (_, y) = frame.local(event.position);
        let start = if y < bar.offset || y > bar.offset + bar.handle {
            (y - bar.handle / 2.).clamp(0., bar.travel)
        } else {
            bar.offset
        };
        host.input.halt();
        host.input.press = None;
        host.input.grab = Some(y - start);
        scroll_to(
            host,
            frame,
            bar.position(frame, start),
            cx.background_executor().now(),
        );
        self.hold_pointer(side, true, window, cx);
        self.focus_pane(side, cx);
        window.focus(&self.focus, cx);
        self.keyboard.context = flea::keys::Context::Listing;
        self.keyboard.disarm();
        self.arm_input_tick(side, window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn grid_bar_move(
        &mut self,
        side: usize,
        event: &MouseMoveEvent,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let blocked = self.input_blocked(cx);
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if blocked
            || host.pane.busy()
            || host.stopped
            || host
                .grid
                .as_ref()
                .is_none_or(|grid| grid.next_view.is_some())
        {
            host.input.grab = None;
            return;
        }
        let Some(frame) = host
            .input
            .frame
            .filter(|frame| frame.listing == host.pane.held.listing)
        else {
            host.input.grab = None;
            return;
        };
        let was = host.input.in_lane;
        host.input.in_lane = hovered && frame.in_lane(event.position);
        if let Some(grab) = host.input.grab {
            let (_, y) = frame.local(event.position);
            scroll_to(
                host,
                frame,
                Bar::new(frame).position(frame, y - grab),
                cx.background_executor().now(),
            );
        }
        if was != host.input.in_lane || host.input.grab.is_some() {
            self.arm_input_tick(side, window, cx);
            cx.notify();
        }
    }

    pub(crate) fn grid_bar_up(
        &mut self,
        side: usize,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let released = event.button == MouseButton::Left
            && self.panes[side].as_mut().is_some_and(|host| {
                let grabbed = host.input.grab.take().is_some();
                if grabbed {
                    host.input.press = None;
                }
                grabbed
            });
        if released {
            self.arm_input_tick(side, window, cx);
            cx.notify();
        }
    }
}
