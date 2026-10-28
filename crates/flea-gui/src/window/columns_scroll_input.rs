//! Lane presses and retained drags write only the directory track which owns the scrollbar.
use crate::{
    columns::geometry::Slot,
    input::{scroll::Bar, Frame},
    state::Shell,
    window::{pane::PaneHost, startup::View},
};
use gpui::{Context, MouseButton, MouseDownEvent, MouseMoveEvent, Window};

fn valid(host: &PaneHost, slot: Slot) -> Option<Frame> {
    let columns = host.columns.as_ref()?;
    let bar = &columns.bars[slot.index()];
    let frame = bar.input.frame?;
    let view = columns
        .area
        .tracks()
        .iter()
        .find(|view| view.slot == slot)?;
    (!host.stopped
        && !host.pane.busy()
        && columns.area.visible
        && view.enabled
        && view.path == bar.path
        && frame.listing == host.pane.held.listing)
        .then_some(frame)
}

fn write(host: &mut PaneHost, slot: Slot, frame: Frame, position: f32) {
    if slot == Slot::Active {
        host.input.halt();
        host.pane.scroll_extent = Some(frame.content);
        host.pane.scroll_to(position);
    } else if let Some(columns) = &mut host.columns {
        let before = columns.area.scroll(slot);
        columns.area.scroll_by(slot, position - before);
    }
}

impl Shell {
    pub(super) fn column_bar_down(
        &mut self,
        side: usize,
        slot: Slot,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.shown_view() != View::Columns
            || self.input_blocked(cx)
            || event.button != MouseButton::Left
        {
            return;
        }
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(frame) = valid(host, slot)
            .filter(|frame| frame.in_lane(event.position) && frame.max_scroll() > 0.5)
        else {
            return;
        };
        let geometry = Bar::new(frame);
        let (_, y) = frame.local(event.position);
        let start = if y < geometry.offset || y > geometry.offset + geometry.handle {
            (y - geometry.handle / 2.).clamp(0., geometry.travel)
        } else {
            geometry.offset
        };
        host.columns.as_mut().unwrap().bars[slot.index()].input.grab = Some(y - start);
        write(host, slot, frame, geometry.position(frame, start));
        self.column_bar_moved(side, slot, cx);
        self.hold_pointer(side, true, window, cx);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn column_bar_move(
        &mut self,
        side: usize,
        slot: Slot,
        event: &MouseMoveEvent,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let blocked = self.input_blocked(cx) || self.shown_view() != View::Columns;
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let frame = valid(host, slot).filter(|_| !blocked);
        let Some(frame) = frame else {
            self.column_bar_up(side, slot, window, cx);
            return;
        };
        let bar = &mut host.columns.as_mut().unwrap().bars[slot.index()];
        let was = bar.input.in_lane;
        bar.input.in_lane = hovered && frame.in_lane(event.position);
        let changed = was != bar.input.in_lane;
        let grab = bar.input.grab;
        if event.pressed_button != Some(MouseButton::Left) {
            self.column_bar_up(side, slot, window, cx);
        } else if let Some(grab) = grab {
            let (_, y) = frame.local(event.position);
            write(host, slot, frame, Bar::new(frame).position(frame, y - grab));
            self.column_bar_moved(side, slot, cx);
        }
        if changed || grab.is_some() {
            cx.notify();
        }
    }

    pub(super) fn column_bar_up(
        &mut self,
        side: usize,
        slot: Slot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let released = self.panes[side]
            .as_mut()
            .and_then(|h| h.columns.as_mut())
            .is_some_and(|c| c.bars[slot.index()].input.grab.take().is_some());
        if released {
            self.hold_pointer(side, false, window, cx);
            cx.notify();
        }
    }
}
