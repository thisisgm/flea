//! Columns and Grid lend their own geometry to the shared selection band without changing row or wheel routing.
use super::Frame;
use crate::{
    columns::geometry::{Slot, LANE},
    grid::Shown,
    list::Palette,
    state::Shell,
    window::{pane::PaneHost, startup::View},
};
use gpui::{point, px, size, Bounds, Entity, IntoElement, Pixels};
use std::{rc::Rc, time::Instant};

fn frame(host: &PaneHost, view: View, bounds: Bounds<Pixels>) -> Option<Frame> {
    let total = host.filter.shown_total(host.pane.held.total);
    let (row, lane, content, scroll, columns, cell_width, cell_origin) = match view {
        View::Grid => {
            let grid = host.grid.as_ref()?;
            if !grid.band_ready() {
                return None;
            }
            let shown = host
                .filter
                .shown
                .as_deref()
                .map_or(Shown::All(total), Shown::Only);
            let layout = grid.view.layout(shown);
            (
                layout.cell_height,
                layout.metrics.pad_x,
                layout.extent(),
                grid.view.drawn_scroll(),
                layout.columns,
                layout.cell_width,
                (layout.metrics.gap, layout.metrics.gap),
            )
        }
        View::Columns => {
            let row = crate::list::pane::ROW_HEIGHT;
            (
                row,
                LANE,
                total as f32 * row,
                host.pane.scroll_y,
                1,
                f32::from(bounds.size.width) - LANE,
                (0., 0.),
            )
        }
        _ => return None,
    };
    Some(Frame {
        bounds,
        row,
        lane,
        content,
        scroll,
        listing: host.pane.held.listing,
        total,
        columns,
        cell_width,
        cell_origin,
    })
}

/// Invalidate geometry before selection facts are captured, then advance the existing band on the view's scroll axis.
pub(crate) fn prepare(host: &mut PaneHost, view: View, mut bounds: Bounds<Pixels>, now: Instant) {
    if host.pane.state.kind != crate::pane::states::Kind::Ready {
        host.input.cancel(&mut host.selected, &mut host.pane.cursor);
        host.input.frame = None;
        host.input.drawn = None;
        host.input.hit_boxes = None;
        return;
    }
    if view == View::Columns {
        let Some(track) = host.columns.as_ref().and_then(|columns| {
            columns
                .area
                .tracks()
                .iter()
                .find(|track| track.slot == Slot::Active)
        }) else {
            return;
        };
        bounds.origin.x += px(track.x);
        bounds.size.width = px(track.width);
    }
    let Some(mut frame) = frame(host, view, bounds) else {
        return;
    };
    host.input
        .prepare(frame, &mut host.selected, &mut host.pane.cursor);
    if host.input.band.is_some() {
        host.input.frame = Some(frame);
        host.pane.scroll_extent = Some(frame.content);
        host.pane.scroll_to(frame.scroll);
        host.input.advance(now, &mut host.pane, &mut host.selected);
        frame.scroll = host.pane.scroll_y;
        host.input.frame = Some(frame);
        if view == View::Grid {
            if let Some(grid) = host.grid.as_mut() {
                grid.view.scroll = frame.scroll;
            }
        }
    } else {
        host.input.frame = None;
    }
}

pub(crate) fn surface(shell: Entity<Shell>, side: usize, palette: Palette) -> impl IntoElement {
    super::element::surface_mode(shell, side, true, move |shell, bounds| {
        let view = shell.shown_view();
        let host = shell.panes[side].as_mut()?;
        let frame = frame(host, view, bounds)?;
        host.input.hit_boxes = if view == View::Grid {
            let grid = host.grid.as_ref()?;
            let state = grid.state.borrow();
            Some(Rc::from(
                state
                    .drawn()
                    .iter()
                    .flat_map(|tile| {
                        let boxes = if tile.editing {
                            [tile.cell; 2]
                        } else {
                            [tile.slot, tile.painted]
                        };
                        boxes.map(|rect| {
                            (
                                tile.listing,
                                Bounds::new(
                                    bounds.origin + point(px(rect.x), px(rect.y)),
                                    size(px(rect.width), px(rect.height)),
                                ),
                            )
                        })
                    })
                    .collect::<Vec<_>>(),
            ))
        } else {
            None
        };
        Some((frame, palette))
    })
}
