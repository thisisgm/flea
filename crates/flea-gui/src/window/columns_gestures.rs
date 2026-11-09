//! Columns lends its painted active track to the shared gestures and resolves drops in every drawn track.
use super::PaneHost;
use crate::{
    columns::{
        geometry::{Slot, NAME_X},
        TrackRows, TrackView,
    },
    state::Shell,
};
use gpui::{div, prelude::*, px, Context, Pixels, Point};
use std::rc::Rc;

pub(super) fn surface(
    side: usize,
    palette: crate::list::Palette,
    cx: &mut Context<Shell>,
) -> impl IntoElement {
    crate::input::surface_with(cx.entity(), side, move |shell, bounds| {
        let grow = shell.grows[side];
        let host = shell.panes[side].as_mut()?;
        let total = host.filter.shown_total(host.pane.held.total);
        let row = host.columns_row_height();
        let lane = crate::columns::geometry::lane(&host.list.borrow().typeface);
        let frame = crate::input::Frame {
            bounds,
            row,
            lane,
            content: crate::columns::geometry::active_content(
                grow,
                total,
                row,
                &host.list.borrow().typeface,
            ),
            scroll: host.pane.scroll_y,
            listing: host.pane.held.listing,
            total,
            columns: 1,
            cell_width: f32::from(bounds.size.width) - lane,
            grow,
            cell_origin: (0., 0.),
        };
        host.input
            .prepare(frame, &mut host.selected, &mut host.pane.cursor);
        host.input.set_shown(host.filter.shown.clone());
        host.input.hit_boxes = None;
        Some((frame, palette))
    })
}

pub(super) fn draggable(
    area: gpui::Div,
    side: usize,
    cx: &mut Context<Shell>,
) -> gpui::Stateful<gpui::Div> {
    use crate::g2::drag_drop::join::{flock, Seed};
    let lifter = cx.entity().downgrade();
    div()
        .id(("columns-file-drag", side))
        .size_full()
        .child(area)
        .capture_any_mouse_down(
            cx.listener(move |shell, event: &gpui::MouseDownEvent, _, _| {
                let Some(host) = shell.panes[side].as_mut() else {
                    return;
                };
                let pressed = (event.button == gpui::MouseButton::Left)
                    .then(|| hover_at(host, event.position))
                    .flatten()
                    .map(|hover| (hover, event.modifiers));
                if let Some(columns) = host.columns.as_mut() {
                    columns.pressed = pressed;
                }
            }),
        )
        .on_click(cx.listener(move |shell, event, window, cx| {
            shell.columns_clicked_file(side, event, window, cx)
        }))
        .on_drag(Seed { side }, move |_, _, window, cx| {
            let _ = lifter.update(cx, |shell, cx| shell.begin_file_drag(side, window, cx));
            flock(cx)
        })
        .map(|div| Shell::own_file_drag(div, side, cx))
}

impl Shell {
    /// A completed neighbour file tap selects it; GPUI cancels this click when the press becomes a drag.
    fn columns_clicked_file(
        &mut self,
        side: usize,
        event: &gpui::ClickEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let gpui::ClickEvent::Mouse(event) = event else {
            return;
        };
        if event.down.click_count != 1
            || self.shown_view() != crate::window::startup::View::Columns
            || self.input_blocked(cx)
            || cx
                .try_global::<crate::g2::file_clipboard::carry::Carrying>()
                .is_some_and(|carrying| carrying.carry.active())
        {
            return;
        }
        let Some(host) = self.panes[side]
            .as_ref()
            .filter(|host| !host.stopped && !host.pane.busy() && !host.nav.gate.owned())
        else {
            return;
        };
        let Some(hover) = hover_at(host, event.up.position) else {
            return;
        };
        let Some(columns) = host.columns.as_ref() else {
            return;
        };
        if hover.slot == Slot::Active
            || columns.pressed.as_ref().map(|(pressed, _)| pressed) != Some(&hover)
        {
            return;
        }
        let Some(view) = columns
            .area
            .tracks()
            .iter()
            .find(|view| view.slot == hover.slot)
        else {
            return;
        };
        let TrackRows::Peek(rows) = &view.rows else {
            return;
        };
        let Some(row) = rows.get(hover.index).filter(|row| !row.dir) else {
            return;
        };
        let (base, name) = (view.path.clone(), row.name.clone());
        self.columns_neighbour_file(side, &base, &name, window, cx);
        self.sync_columns(window, cx);
        cx.stop_propagation();
        cx.notify();
    }
}

pub(crate) fn on_name(host: &PaneHost, row: usize, x: Pixels) -> bool {
    let Some(columns) = &host.columns else {
        return false;
    };
    let Some(paint) = columns.painters.get(Slot::Active) else {
        return false;
    };
    let paint = paint.borrow();
    let Some(drawn) = &paint.drawn else {
        return false;
    };
    let Some(prepared) = paint.prepared(row) else {
        return false;
    };
    let left = drawn.bounds.left() + px(NAME_X);
    let width = prepared
        .name(prepared.dir && host.pane.cursor == row)
        .line()
        .width;
    x >= left && x < left + width
}

/// A hovered row, read from its track's path and rows and the active listing: a swap of any of them drops it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Hover {
    pub slot: Slot,
    pub index: usize,
    path: String,
    rows: TrackRows,
    listing: usize,
}

impl Hover {
    /// Whether the hovered track still shows the same path and rows, under the same active listing.
    pub(crate) fn holds(&self, tracks: &Rc<[TrackView]>, listing: usize) -> bool {
        self.listing == listing
            && tracks.iter().any(|view| {
                view.slot == self.slot && view.path == self.path && view.rows == self.rows
            })
    }
}

impl super::Columns {
    /// A blocked pane or replaced track drops its hover before the shared rows and track painter read it.
    pub(super) fn settle_hover(&mut self, listing: usize, blocked: bool) {
        if blocked
            || self
                .hover
                .as_ref()
                .is_some_and(|hover| !hover.holds(self.area.tracks(), listing))
        {
            self.hover = None;
        }
    }
}

/// The row under the pointer on a drawn track, never the lane, a gap below the rows or a hidden listing.
pub(crate) fn hover_at(host: &PaneHost, point: Point<Pixels>) -> Option<Hover> {
    let columns = host.columns.as_ref()?;
    let tracks = columns.area.tracks();
    tracks.iter().enumerate().find_map(|(at, view)| {
        let paint = columns.painters.get(view.slot)?.borrow();
        let drawn = paint.drawn.as_ref()?;
        if !view.enabled
            || !Rc::ptr_eq(&drawn.tracks, tracks)
            || drawn.at != at
            || !drawn.bounds.contains(&point)
            || point.x >= drawn.bounds.right() - px(drawn.lane)
            || matches!(view.rows, TrackRows::Preview)
            || drawn
                .held
                .as_ref()
                .is_none_or(|held| held.listing != host.pane.held.listing)
        {
            return None;
        }
        let position = ((f32::from(point.y - drawn.bounds.top()) + drawn.scroll) / drawn.row_height)
            .floor() as usize;
        let index = drawn.row_at(position)?;
        crate::columns::element::row_at(&view.rows, drawn.held.as_deref(), index).map(|_| Hover {
            slot: view.slot,
            index,
            path: view.path.clone(),
            rows: view.rows.clone(),
            listing: host.pane.held.listing,
        })
    })
}

pub(crate) fn spring_spot(
    host: &PaneHost,
    point: Point<Pixels>,
    carried: &[usize],
) -> Option<String> {
    destination(host, point, carried, true)
}

pub(crate) fn drop_spot(
    host: &PaneHost,
    point: Point<Pixels>,
    carried: &[usize],
) -> Option<String> {
    destination(host, point, carried, false)
}

fn destination(
    host: &PaneHost,
    point: Point<Pixels>,
    carried: &[usize],
    spring: bool,
) -> Option<String> {
    let columns = host.columns.as_ref()?;
    let tracks = columns.area.tracks();
    tracks.iter().enumerate().find_map(|(at, view)| {
        let paint = columns.painters.get(view.slot)?.borrow();
        let drawn = paint.drawn.as_ref()?;
        if !view.enabled
            || !Rc::ptr_eq(&drawn.tracks, tracks)
            || drawn.at != at
            || !drawn.bounds.contains(&point)
            || matches!(view.rows, TrackRows::Preview)
            || drawn
                .held
                .as_ref()
                .is_none_or(|held| held.listing != host.pane.held.listing)
        {
            return None;
        }
        let position = drawn.grow.position_at(
            f32::from(point.y - drawn.bounds.top()) + drawn.scroll,
            drawn.row_height,
        );
        let row = (point.x < drawn.bounds.right() - px(drawn.lane))
            .then_some(drawn.row_at(position))
            .flatten();
        if view.slot == Slot::Active {
            let dest = crate::g2::drag_drop::join::row_spot(host, row, carried);
            return (!spring || dest != host.path).then_some(dest);
        }
        let target = match &view.rows {
            TrackRows::Peek(rows) => row
                .and_then(|row| rows.get(row))
                .filter(|row| row.dir)
                .map(|row| crate::navigation::history::join(&view.path, &row.name)),
            _ => None,
        };
        target.or_else(|| (!spring).then(|| view.path.clone()))
    })
}
