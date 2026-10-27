//! Columns lends its painted active track to the shared gestures and resolves drops in every drawn track.
use super::PaneHost;
use crate::{
    columns::{
        geometry::{Slot, LANE, NAME_X},
        TrackRows,
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
        let host = shell.panes[side].as_mut()?;
        let total = host.filter.shown_total(host.pane.held.total);
        let frame = crate::input::Frame {
            bounds,
            row: crate::list::pane::ROW_HEIGHT,
            lane: LANE,
            content: total as f32 * crate::list::pane::ROW_HEIGHT,
            scroll: host.pane.scroll_y,
            listing: host.pane.held.listing,
            total,
            columns: 1,
            cell_width: f32::from(bounds.size.width) - LANE,
            cell_origin: (0., 0.),
        };
        host.input
            .prepare(frame, &mut host.selected, &mut host.pane.cursor);
        host.input.shown = host.filter.shown.clone();
        host.input.hit_boxes = None;
        Some((frame, palette))
    })
}

pub(super) fn draggable(
    area: gpui::Div,
    side: usize,
    cx: &mut Context<Shell>,
) -> gpui::Stateful<gpui::Div> {
    use crate::g2::dg2::join::{flock, Seed};
    let lifter = cx.entity().downgrade();
    let payload = cx.entity().downgrade();
    div()
        .id(("columns-file-drag", side))
        .size_full()
        .child(area)
        .on_drag(Seed { side }, move |_, _, window, cx| {
            let _ = lifter.update(cx, |shell, cx| shell.begin_file_drag(side, window, cx));
            flock(cx)
        })
        .external_drag_payload::<Seed>(move |_, window, cx| {
            payload.upgrade()?.read(cx).file_drag_payload(window, cx)
        })
        .on_drop::<Seed>(
            cx.listener(move |shell, _: &Seed, window, cx| shell.land_own_drag(side, window, cx)),
        )
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

/// The row under the pointer on a drawn track, never the lane, a gap below the rows or a hidden listing.
pub(crate) fn hover_at(host: &PaneHost, point: Point<Pixels>) -> Option<(Slot, usize)> {
    let columns = host.columns.as_ref()?;
    let tracks = columns.area.tracks();
    tracks.iter().enumerate().find_map(|(at, view)| {
        let paint = columns.painters.get(view.slot)?.borrow();
        let drawn = paint.drawn.as_ref()?;
        if !view.enabled
            || !Rc::ptr_eq(&drawn.tracks, tracks)
            || drawn.at != at
            || !drawn.bounds.contains(&point)
            || point.x >= drawn.bounds.right() - px(LANE)
            || matches!(view.rows, TrackRows::Preview)
            || drawn
                .held
                .as_ref()
                .is_none_or(|held| held.listing != host.pane.held.listing)
        {
            return None;
        }
        let position = ((f32::from(point.y - drawn.bounds.top()) + drawn.scroll)
            / crate::list::pane::ROW_HEIGHT)
            .floor() as usize;
        let index = drawn.row_at(position)?;
        crate::columns::element::row_at(&view.rows, drawn.held.as_deref(), index)
            .map(|_| (view.slot, index))
    })
}

pub(crate) fn drop_spot(
    host: &PaneHost,
    point: Point<Pixels>,
    carried: &[usize],
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
        let position = ((f32::from(point.y - drawn.bounds.top()) + drawn.scroll)
            / crate::list::pane::ROW_HEIGHT)
            .floor() as usize;
        let row = (point.x < drawn.bounds.right() - px(LANE))
            .then_some(drawn.row_at(position))
            .flatten();
        if view.slot == Slot::Active {
            return Some(crate::g2::dg2::join::row_spot(host, row, carried));
        }
        let target = match &view.rows {
            TrackRows::Peek(rows) => row
                .and_then(|row| rows.get(row))
                .filter(|row| row.dir)
                .map(|row| crate::navigation::history::join(&view.path, &row.name)),
            _ => None,
        };
        Some(target.unwrap_or_else(|| view.path.clone()))
    })
}
