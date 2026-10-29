//! Wheel and lane gestures write only the painted track they own.
use super::super::pane::PaneHost;
use crate::{
    columns::{geometry::Slot, TrackRows},
    input::scroll::Bar,
    state::Shell,
};
use gpui::{
    Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ScrollWheelEvent, Window,
};

pub(super) fn write(host: &mut PaneHost, slot: Slot, y: f32) {
    if slot == Slot::Active {
        host.pane.scroll_to(y);
    } else if let Some(columns) = &mut host.columns {
        let before = columns.area.scroll(slot);
        columns.area.scroll_by(slot, y - before);
    }
}

impl Shell {
    pub(crate) fn columns_scroll_hold(&mut self, side: usize, position: gpui::Point<gpui::Pixels>) {
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let Some(columns) = &mut host.columns else {
            return;
        };
        let slot = columns.area.tracks().iter().find_map(|view| {
            let drawn = columns.painters.get(view.slot)?.borrow();
            drawn
                .drawn
                .as_ref()?
                .bounds
                .contains(&position)
                .then_some(view.slot)
        });
        let Some(slot) = slot else { return };
        if let Some(to) = columns.scroll[slot.index()].hold() {
            write(host, slot, to)
        }
    }

    pub(crate) fn columns_scroll_wheel(
        &mut self,
        side: usize,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        if self.view != super::super::startup::View::Columns {
            return;
        }
        let now = cx.background_executor().now();
        let reduced = cx.reduce_motion();
        let blocked = self.wheel_blocked(cx);
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(columns) = &mut host.columns else {
            return;
        };
        let slot = columns.area.tracks().iter().find_map(|view| {
            let paint = columns.painters.get(view.slot)?.borrow();
            let drawn = paint.drawn.as_ref()?;
            (std::rc::Rc::ptr_eq(&drawn.tracks, columns.area.tracks())
                && drawn.bounds.contains(&event.position)
                && !matches!(view.rows, TrackRows::Preview)
                && view.enabled)
                .then_some(view.slot)
        });
        let Some(slot) = slot else { return };
        let state = &mut columns.scroll[slot.index()];
        // A card's own surface stops the wheel above it, so a card beside the track never does.
        if blocked || host.pane.busy() || state.input.grab.is_some() {
            state.input.halt();
            state.input.stroke = false;
            return;
        }
        if state.wheel(event, now, reduced) {
            cx.stop_propagation();
        }
        if let Some(frame) = state.input.frame {
            write(host, slot, frame.scroll);
        }
        cx.notify();
    }

    pub(crate) fn columns_bar_down(
        &mut self,
        side: usize,
        slot: Slot,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view != super::super::startup::View::Columns {
            return;
        }
        if event.button != MouseButton::Left || self.input_blocked(cx) {
            return;
        }
        let Some(host) = self.panes[side].as_mut().filter(|host| !host.pane.busy()) else {
            return;
        };
        let Some(columns) = &mut host.columns else {
            return;
        };
        let state = &mut columns.scroll[slot.index()];
        let Some(frame) = state
            .input
            .frame
            .filter(|f| f.in_lane(event.position) && f.max_scroll() > 0.5)
        else {
            return;
        };
        state.input.halt();
        let bar = Bar::new(frame);
        let (_, y) = frame.local(event.position);
        let grab = if y >= bar.offset && y <= bar.offset + bar.handle {
            y - bar.offset
        } else {
            bar.handle / 2.
        };
        state.input.grab = Some(grab);
        state.input.moved(cx.background_executor().now());
        let to = bar.position(frame, y - grab);
        state.input.wrote = Some(to);
        state.input.frame.as_mut().unwrap().scroll = to;
        write(host, slot, to);
        self.hold_pointer(side, true, window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(crate) fn columns_bar_move(
        &mut self,
        side: usize,
        slot: Slot,
        event: &MouseMoveEvent,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let blocked = self.input_blocked(cx);
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(columns) = &mut host.columns else {
            return;
        };
        let state = &mut columns.scroll[slot.index()];
        let Some(frame) = state.input.frame else {
            return;
        };
        let lane = hovered && frame.in_lane(event.position);
        let mut changed = lane != state.input.in_lane;
        state.input.in_lane = lane;
        if let Some(grab) = state.input.grab {
            if blocked || host.pane.busy() || event.pressed_button != Some(MouseButton::Left) {
                state.input.grab = None;
                self.hold_pointer(side, false, window, cx);
                cx.notify();
                return;
            }
            let (_, y) = frame.local(event.position);
            let to = Bar::new(frame).position(frame, y - grab);
            state.input.moved(cx.background_executor().now());
            state.input.wrote = Some(to);
            state.input.frame.as_mut().unwrap().scroll = to;
            write(host, slot, to);
            changed = true;
            cx.stop_propagation();
        }
        if changed {
            cx.notify()
        }
    }

    pub(crate) fn columns_bar_up(
        &mut self,
        side: usize,
        slot: Slot,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }
        let Some(state) = self.panes[side]
            .as_mut()
            .and_then(|h| h.columns.as_mut())
            .map(|c| &mut c.scroll[slot.index()])
        else {
            return;
        };
        let grabbed = state.input.grab.take().is_some();
        state.release(cx.background_executor().now(), cx.reduce_motion());
        if state.input.moving() {
            cx.notify()
        }
        if grabbed {
            self.hold_pointer(side, false, window, cx);
            cx.stop_propagation();
            cx.notify();
        }
    }
}
