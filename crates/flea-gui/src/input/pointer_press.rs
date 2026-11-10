//! Pointer presses hold the current motion before choosing a row, scrollbar or selection band.
use super::{pointer::Press, scroll::Bar, Band};
use crate::state::Shell;
use gpui::{Context, MouseButton, MouseDownEvent, Window};

impl Shell {
    pub(crate) fn input_blocked(&self, cx: &gpui::App) -> bool {
        self.wheel_blocked(cx) || self.jobs_card_showing(cx)
    }
    /// `input_blocked` without the Jobs card: a wheel beside that non-modal card still moves its track.
    pub(crate) fn wheel_blocked(&self, cx: &gpui::App) -> bool {
        self.lifecycle.phase() != crate::window::lifecycle::Phase::Open
            || self.modal_card_holds_keyboard(cx)
            || self.menu_opened()
            || self.menu.presentation.at.is_some()
            || self.input.blocked
            || self.panes[self.focus_side]
                .as_ref()
                .is_some_and(|host| host.search.kept.is_some())
    }
    pub(crate) fn pointer_down(
        &mut self,
        side: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A press takes the keyboard back from an Enter still waiting on its rows, as any key does.
        for host in self.panes.iter_mut().flatten() {
            host.filter.enter_after = None;
            host.search.enter_after = None;
        }
        if self.input_blocked(cx) {
            return;
        }
        let outline = self.panes[side]
            .as_ref()
            .and_then(|host| host.input.hit(event.position))
            .filter(|&row| self.outline_hit(side, row, event.position.x));
        let columns_shown = self.shown_view() == crate::window::startup::View::Columns;
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        let Some(frame) = host.input.frame else {
            return;
        };
        if host.pane.busy() {
            let text = self.locale.catalog.tr("pane.busy");
            self.status_note(&text, cx);
            return;
        }
        if frame.listing != host.pane.held.listing {
            return;
        }
        // A directory column owns its lane; let its overlay receive this press.
        if columns_shown && frame.in_lane(event.position) {
            return;
        }
        host.input.keyboard();
        if event.button == MouseButton::Left && outline.is_some() {
            host.input.pointer_origin = true;
            host.input.hovered = outline;
        }
        // Every press starts its own gesture, so no earlier press can lift rows for this one.
        host.input.press = None;
        host.input.hold(&mut host.pane);
        // The hold may land a glide where it is drawn, so the row and band read the frame it left.
        let frame = host.input.frame.unwrap_or(frame);
        // Every press starts its own gesture, so no earlier press can lift rows for this one.
        host.input.press = None;
        if self.carry_list_press(side, frame, event, window, cx) {
            return;
        }
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        if frame.in_lane(event.position)
            && frame.max_scroll() > 0.5
            && event.button == MouseButton::Left
            && !columns_shown
        {
            let bar = Bar::new(frame);
            let (_, y) = frame.local(event.position);
            let start = if y < bar.offset || y > bar.offset + bar.handle {
                (y - bar.handle / 2.).clamp(0., bar.travel)
            } else {
                bar.offset
            };
            host.pane.scroll_to(bar.position(frame, start));
            host.input.grab = Some(y - start);
            if host.pane.scroll_y != frame.scroll {
                host.input
                    .scrolled(host.pane.scroll_y, &mut host.pane.cursor);
                host.input.moved(cx.background_executor().now());
            }
        } else if let Some(row) = host.input.hit(event.position) {
            self.rename.slow_press();
            host.input.press = Some(Press {
                row,
                listing: frame.listing,
                name: host.pane.held.row(row).map(|row| row.name.clone()),
                event: event.clone(),
                dragged: false,
            });
        } else if event.button == MouseButton::Left && !frame.in_lane(event.position) {
            // SelectionBand.qml: a ground press ends the pending slow click, or its timer renames the sole selected row.
            self.rename.slow = None;
            let (x, y) = frame.local(event.position);
            host.input.band = Some(Band {
                start: event.position,
                pointer: event.position,
                anchor: (x, y + frame.scroll),
                end: (x, y + frame.scroll),
                before: host.selected.clone(),
                cursor: host.pane.cursor,
                active: false,
                additive: event.modifiers.control,
                shift: event.modifiers.shift,
                direction: 0.,
                last: None,
                cells: None,
                basis: None,
            });
        }
        // A row press bubbles on to the pane's on_drag, which lifts the row once the press travels.
        let row_press = event.button == MouseButton::Left
            && self.panes[side]
                .as_ref()
                .is_some_and(|host| host.input.press.is_some());
        self.focus_pane(side, cx);
        window.focus(&self.focus, cx);
        self.keyboard.context = flea::keys::Context::Listing;
        self.keyboard.disarm();
        if !row_press {
            cx.stop_propagation();
        }
        cx.notify();
    }
}
