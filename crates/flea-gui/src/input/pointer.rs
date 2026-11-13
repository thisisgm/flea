//! Native pointer delivery owns the gesture until release and never retargets it after a listing swap.
use super::scroll::Bar;
use crate::{
    state::Shell,
    window::{
        dispatch::Order,
        input::{pointer, ActionEvent},
    },
};
use gpui::{Context, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window};

pub struct Press {
    pub row: usize,
    pub listing: usize,
    pub name: Option<String>,
    pub event: MouseDownEvent,
    pub dragged: bool,
}

pub struct Tap {
    listing: usize,
    row: usize,
    name: Option<String>,
    base: usize,
}
impl super::PaneInput {
    fn tap_count(&mut self, press: &Press) -> usize {
        let count = press.event.click_count;
        let same = self.tap.as_ref().is_some_and(|tap| {
            tap.listing == press.listing && tap.row == press.row && tap.name == press.name
        });
        if count == 1 || self.tap.is_none() {
            self.tap = Some(Tap {
                listing: press.listing,
                row: press.row,
                name: press.name.clone(),
                base: 0,
            });
            count
        } else if same {
            count.saturating_sub(self.tap.as_ref().expect("first tap exists").base)
        } else {
            self.tap = Some(Tap {
                listing: press.listing,
                row: press.row,
                name: press.name.clone(),
                base: count,
            });
            0
        }
    }
}

impl Shell {
    pub(crate) fn pointer_move(
        &mut self,
        side: usize,
        event: &MouseMoveEvent,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let blocked = self.input_blocked(cx);
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        if blocked || (host.pane.busy() && !host.pane.swap.retains_rows()) {
            host.input.cancel(&mut host.selected, &mut host.pane.cursor);
            host.input.hovered = None;
            if let Some(columns) = host.columns.as_mut() {
                columns.hover = None;
            }
            return;
        }
        if host.pane.busy() {
            return;
        }
        let Some(mut frame) = host.input.frame else {
            return;
        };
        let moved = host.input.last_pointer != Some(event.position);
        host.input.last_pointer = Some(event.position);
        let before = (
            host.input.in_lane,
            host.input.hovered,
            host.input.pointer_origin,
            host.columns
                .as_ref()
                .and_then(|columns| columns.hover.clone()),
        );
        host.input.in_lane = hovered && frame.in_lane(event.position);
        if let Some(press) = &mut host.input.press {
            let delta = event.position - press.event.position;
            if f32::from(delta.x).powi(2) + f32::from(delta.y).powi(2) >= 100. {
                press.dragged = true;
            }
        }
        if let Some(grab) = host.input.grab {
            let (_, y) = frame.local(event.position);
            host.pane
                .scroll_to(Bar::new(frame).position(frame, y - grab));
            if host.pane.scroll_y != frame.scroll {
                host.input
                    .scrolled(host.pane.scroll_y, &mut host.pane.cursor);
                host.input.moved(cx.background_executor().now());
            }
        } else if let Some(band) = &mut host.input.band {
            band.pointer = event.position;
            let delta = event.position - band.start;
            if f32::from(delta.x).powi(2) + f32::from(delta.y).powi(2) >= 100. {
                band.active = true;
            }
            frame.scroll = host.pane.scroll_y;
            let before = host.selected.len();
            let touched =
                super::update_band(frame, band, &mut host.selected, host.input.shown.as_deref());
            if touched > 0 || before != host.selected.len() {
                host.input.selection.changed(&host.selected);
            }
        } else if moved {
            host.input.pointer_origin = true;
            host.input.surface_pointer = hovered.then_some(event.position);
            host.input.hovered = hovered.then(|| host.input.hit(event.position)).flatten();
            // The gesture surface sits on the active track only, so a peek track's row reads the window's move by its bounds.
            let hover = crate::window::columns::hover_at(host, event.position);
            if let Some(columns) = host.columns.as_mut() {
                columns.hover = hover;
            }
        }
        // Only a changed hover, lane or live gesture draws differently, so other motion repaints nothing.
        let drawn = before
            != (
                host.input.in_lane,
                host.input.hovered,
                host.input.pointer_origin,
                host.columns
                    .as_ref()
                    .and_then(|columns| columns.hover.clone()),
            )
            || host.input.grab.is_some()
            || host.input.band.is_some();
        if drawn {
            self.arm_input_tick(side, window, cx);
            cx.notify();
        }
    }
    pub(crate) fn pointer_up(
        &mut self,
        side: usize,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pointer_release(side, cx);
        if event.button != MouseButton::Left
            && self.panes[side].as_ref().is_some_and(|host| {
                host.input
                    .press
                    .as_ref()
                    .is_none_or(|press| press.event.button != event.button)
            })
        {
            return;
        }
        let blocked = self.input_blocked(cx);
        let outline = self.panes[side]
            .as_ref()
            .and_then(|host| host.input.press.as_ref())
            .filter(|press| {
                self.outline_hit(side, press.row, press.event.position.x)
                    && self.outline_hit(side, press.row, event.position.x)
            })
            .map(|press| press.row);
        let Some(host) = &mut self.panes[side] else {
            return;
        };
        host.input.grab = None;
        if blocked || host.pane.busy() {
            host.input.cancel(&mut host.selected, &mut host.pane.cursor);
            host.input.press = None;
            return;
        }
        if let Some(mut band) = host.input.band.take() {
            if let Some(mut frame) = host.input.frame {
                frame.scroll = host.pane.scroll_y;
                band.pointer = event.position;
                super::update_band(
                    frame,
                    &mut band,
                    &mut host.selected,
                    host.input.shown.as_deref(),
                );
            }
            if band.active {
                if let Some(last) = band.last {
                    host.pane.cursor = last;
                }
            } else if !band.additive && !band.shift {
                host.input.selection.ground(&mut host.selected);
            }
            host.input.selection.changed(&host.selected);
            host.input.pulled = None;
            host.nav.landed = None;
        }
        let press = host.input.press.take();
        let mut slow = None;
        if let Some(mut press) =
            press.filter(|press| !press.dragged && press.listing == host.pane.held.listing)
        {
            if host.input.hit(event.position) != Some(press.row)
                || host.pane.held.row(press.row).map(|row| row.name.as_str())
                    != press.name.as_deref()
            {
                return;
            }
            let count = host.input.tap_count(&press);
            if count == 0 {
                return;
            }
            if event.button == MouseButton::Left && outline == Some(press.row) {
                let open = !host.pane.held.row(press.row).is_some_and(|row| row.open);
                self.outline_action(
                    side,
                    open,
                    crate::window::outline::Input::Mark(press.row),
                    window,
                    cx,
                );
                cx.stop_propagation();
                return;
            }
            press.event.click_count = count;
            let single_click = self.launch.settings.single_click_open();
            if event.button == MouseButton::Left && single_click && count != 1 {
                return;
            }
            let row = press.row;
            let kind = if host.search.owner.session.reveals_on_activate() {
                "result"
            } else if host.pane.held.row(row).is_some_and(|facts| facts.dir) {
                "dir"
            } else {
                "file"
            };
            let column = self.view == crate::window::startup::View::Columns
                && kind == "dir"
                && event.button == MouseButton::Left
                && !press.event.modifiers.control
                && !press.event.modifiers.shift;
            if column && count != 1 {
                return;
            }
            let effect = pointer(
                if column { "column" } else { "listing" },
                kind,
                &press.event,
            )
            .map(|binding| binding.effect);
            if matches!(effect, None | Some("nothing")) {
                return;
            }
            let from = host.pane.cursor;
            let was_sole = super::slow::was_sole_selection(
                host.selected.len(),
                host.selected.binary_search(&row).is_ok(),
                from == row,
            );
            let pulled = host.input.take_pull(from, host.pane.held.listing);
            let selection = &mut host.input.selection;
            let mut marked = true;
            let action = match effect {
                Some("toggleSelect") if press.event.click_count == 1 => {
                    if host.selected.is_empty() && from != row {
                        selection.toggle(&mut host.selected, from);
                    }
                    selection.toggle(&mut host.selected, row);
                    None
                }
                Some("extendSelect") if press.event.click_count == 1 => {
                    selection.extend(&mut host.selected, from, row, pulled, &host.input.shown);
                    None
                }
                Some("toggleSelect" | "extendSelect") => {
                    marked = false;
                    None
                }
                Some("menu") => {
                    marked =
                        !host.selected.is_empty() && host.selected.binary_search(&row).is_err();
                    if marked {
                        selection.clear(&mut host.selected);
                    }
                    Some("menu")
                }
                Some("openTab") => {
                    marked = false;
                    Some("openTab")
                }
                Some("reveal") => {
                    marked = false;
                    Some("reveal")
                }
                Some("selectOnly" | "open") => {
                    selection.only(&mut host.selected, row);
                    (effect == Some("open") || single_click).then_some("open")
                }
                _ => {
                    marked = false;
                    None
                }
            };
            // v0.3.12 Selection.js:32-35, 47-67: only toggle, only, extendTo, all and clear drop the landing.
            if marked {
                host.nav.landed = None;
            }
            if effect != Some("openTab") {
                host.pane.cursor = row;
                if let Some(mut frame) = host
                    .input
                    .frame
                    .filter(|_| self.view != crate::window::startup::View::Grid)
                {
                    frame.scroll = host.pane.scroll_y;
                    host.pane.scroll_to(super::scroll::reveal(
                        frame,
                        host.input.view_index(row),
                        0,
                    ));
                }
            }
            host.input.pulled = None;
            // List.qml: only a lone left tap on the drawn name can arm the slow click.
            let on_name = event.button == MouseButton::Left
                && count == 1
                && if self.view == crate::window::startup::View::Columns {
                    crate::window::columns::on_name(host, row, event.position.x)
                } else if self.view == crate::window::startup::View::Grid {
                    super::views::grid_on_name(host, row, event.position)
                } else {
                    host.list.borrow().on_name(row, event.position.x)
                };
            slow = Some(crate::window::rename::slow::Tap {
                row,
                on_name,
                modifiers: press.event.modifiers,
                was_sole,
            });
            if let Some(action) = action {
                let event = ActionEvent {
                    action,
                    context: flea::keys::Context::Listing,
                    seat: flea::keys::sequence::Seat {
                        listing: host.pane.held.listing as u64,
                        cursor: row,
                        view: self.view as u64,
                        focus: side as u64,
                        ..Default::default()
                    },
                    pair_armed: false,
                };
                if action == "reveal" {
                    self.focus_side = side;
                    self.search_action("reveal", window, cx);
                } else {
                    self.dispatch(Order::Action(event), window, cx);
                }
            }
        }
        if let Some(tap) = slow {
            self.slow_tap(side, tap, window, cx);
        }
        self.arm_input_tick(side, window, cx);
        cx.notify();
    }
}

#[cfg(test)]
#[path = "pointer_tests.rs"]
mod tests;
