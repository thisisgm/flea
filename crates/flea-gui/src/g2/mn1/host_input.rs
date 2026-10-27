//! The menu's input layer occludes the listing and keeps resting-pointer opens on the first row.
use super::{element::MenuElement, menu::Outcome};
use crate::{
    input::PLATFORM_SCROLL_GAIN, pane::states::Kind, state::Shell, window::startup::View,
};
use gpui::{
    div, prelude::*, px, App, Context, Div, Entity, MouseButton, MouseDownEvent, MouseMoveEvent,
    ScrollDelta, ScrollWheelEvent, TouchPhase, Window,
};

pub(crate) fn layer(shell: &Entity<Shell>, window: &Window, cx: &mut App) -> Option<Div> {
    let state = shell.read(cx);
    if !state.menu_opened() && state.menu.presentation.at.is_none() {
        return None;
    }
    let element = MenuElement::new(state.menu.menu.clone(), px(state.corner));
    let loading = state.menu.presentation.loading.then(|| {
        let at = state.menu.presentation.at.unwrap_or_default();
        let viewport = window.viewport_size();
        let side = crate::feedback::spinner::HERO;
        div()
            .absolute()
            .left(px(at
                .0
                .clamp(0., (f32::from(viewport.width) - side).max(0.))))
            .top(px(at
                .1
                .clamp(0., (f32::from(viewport.height) - side).max(0.))))
            .child(crate::feedback::spinner::Slot(
                state.menu.presentation.mark.clone(),
            ))
    });
    let left = shell.clone();
    let right = shell.clone();
    let hover = shell.clone();
    let wheel = shell.clone();
    Some(
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                left.update(cx, |shell, cx| shell.menu_click(event, window, cx))
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                right.update(cx, |shell, cx| shell.menu_click(event, window, cx))
            })
            .on_mouse_move(move |event, window, cx| {
                hover.update(cx, |shell, cx| shell.menu_hover(event, window, cx))
            })
            .on_scroll_wheel(move |event, window, cx| {
                wheel.update(cx, |shell, cx| {
                    shell.menu_wheel(event, window, cx);
                })
            })
            .child(element)
            .children(loading),
    )
}

impl Shell {
    pub(crate) fn menu_pointer_open(
        &mut self,
        side: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_blocked(cx)
            || (side == 1 && self.view != View::Dual)
            || self.slots.covers[side].is_some()
        {
            return;
        }
        let Some(host) = &self.panes[side] else {
            return;
        };
        let row = match host.drawn_view {
            // An empty folder draws the hero and no row surface, so its whole pane is blank ground.
            Some(View::List | View::Dual)
                if !host.drawn_list && host.pane.state.kind == Kind::Empty =>
            {
                let pane = self.geometry(window).rows(
                    side,
                    self.view == View::Dual,
                    self.slots.strip_height,
                );
                let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
                if x < pane.x || x >= pane.x + pane.width || y < pane.y || y >= pane.y + pane.height
                {
                    return;
                }
                None
            }
            Some(View::List | View::Dual) => {
                let Some(frame) = host
                    .input
                    .drawn
                    .map(|drawn| drawn.frame)
                    .or(host.input.frame)
                else {
                    return;
                };
                if frame.listing != host.pane.held.listing
                    || !frame.bounds.contains(&event.position)
                    || frame.in_lane(event.position)
                {
                    return;
                }
                frame
                    .row_at(event.position)
                    .map(|row| host.input.index(row))
            }
            Some(View::Grid) => {
                let Some(grid) = &host.grid else { return };
                let state = grid.state.borrow();
                if state
                    .frame_held
                    .as_ref()
                    .is_none_or(|held| held.listing != host.pane.held.listing)
                    || !state.bounds.contains(&event.position)
                {
                    return;
                }
                let at = event.position - state.bounds.origin;
                state
                    .tile_at(f32::from(at.x), f32::from(at.y))
                    .and_then(|hit| {
                        state
                            .drawn()
                            .iter()
                            .find(|drawn| drawn.view == hit)
                            .map(|drawn| drawn.listing)
                    })
            }
            _ => return,
        };
        if let Some(row) = row {
            if let Some(host) = &mut self.panes[side] {
                if !host.selected.contains(&row) {
                    host.selected.clear();
                }
            }
        }
        self.open_menu(
            side,
            row,
            (event.position.x.into(), event.position.y.into()),
            window,
            cx,
        );
        cx.stop_propagation();
    }

    pub(crate) fn menu_keyboard_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.focus_side;
        let Some(host) = &self.panes[side] else {
            return;
        };
        if self.input_blocked(cx) {
            return;
        }
        let geometry = self.geometry(window);
        let row = (host.pane.held.total > 0).then_some(host.pane.cursor);
        let pane = geometry.rows(side, self.view == View::Dual, self.slots.strip_height);
        let y = pane.y + host.pane.cursor as f32 * host.pane.row_height() - host.pane.scroll_y
            + host.pane.row_height() / 2.;
        // In Columns the cursor row is in the active column, so the menu opens over that column's row.
        let at = self
            .columns_cursor_anchor(side)
            .unwrap_or((pane.x + 14., y));
        self.open_menu(side, row, at, window, cx);
    }

    fn menu_click(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.button == MouseButton::Right || self.menu.presentation.at.is_some() {
            self.retire_menu(window, cx);
        } else if let Some(capture) = &self.menu.captured {
            let list = self.panes[capture.side]
                .as_ref()
                .expect("the menu's pane")
                .list
                .borrow();
            let live = self.menu_entries();
            let outcome = self.menu.menu.borrow_mut().click(
                event.position.x.into(),
                event.position.y.into(),
                Some(&live),
                &self.menu_identity(),
                &self.menu_env(&list.typeface),
                window,
            );
            drop(list);
            self.menu_outcome(outcome, window, cx);
        }
        cx.stop_propagation();
    }

    fn menu_hover(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let point = (f32::from(event.position.x), f32::from(event.position.y));
        let moved = self.menu.pointer.is_some_and(|old| {
            if self.menu.pointer_armed {
                point != old
            } else {
                (point.0 - old.0).abs() >= 1. || (point.1 - old.1).abs() >= 1.
            }
        });
        self.menu.pointer_armed = true;
        self.menu.pointer = Some(point);
        self.menu.resting = Some(point);
        if moved {
            if let Some(capture) = &self.menu.captured {
                let list = self.panes[capture.side]
                    .as_ref()
                    .expect("the menu's pane")
                    .list
                    .borrow();
                let outcome = self.menu.menu.borrow_mut().hover(
                    point.0,
                    point.1,
                    &self.menu_env(&list.typeface),
                    window,
                );
                drop(list);
                self.menu_outcome(outcome, window, cx);
            }
        }
        cx.stop_propagation();
    }

    pub(crate) fn menu_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.menu_opened() && self.menu.presentation.at.is_none() {
            return false;
        }
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        let outcome = match event.delta {
            ScrollDelta::Lines(lines) => self.menu.menu.borrow_mut().wheel(
                x,
                y,
                (lines.y * 120. / PLATFORM_SCROLL_GAIN).round() as i32,
            ),
            ScrollDelta::Pixels(pixels) => self.menu.menu.borrow_mut().touchpad(
                x,
                y,
                f32::from(pixels.y) / PLATFORM_SCROLL_GAIN,
                event.touch_phase == TouchPhase::Started,
            ),
        };
        if outcome != Outcome::Nothing {
            self.menu_outcome(outcome, window, cx);
        }
        cx.stop_propagation();
        true
    }
}
