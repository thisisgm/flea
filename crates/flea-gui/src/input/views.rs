//! Columns and Grid feed the shared selection band from the frame their own mounted viewport publishes.
use super::Frame;
use crate::{
    state::Shell,
    window::{pane::PaneHost, startup::View},
};
use gpui::{point, px, size, Bounds, Context, MouseButton, MouseDownEvent, Pixels};
use std::{rc::Rc, time::Instant};

/// Advance a live band on the view's scroll axis; the mounted surface owns the frame and its invalidation.
pub(crate) fn prepare(host: &mut PaneHost, view: View, now: Instant) {
    if host.pane.state.kind != crate::pane::states::Kind::Ready {
        host.input.cancel(&mut host.selected, &mut host.pane.cursor);
        return;
    }
    let Some(mut frame) = host.input.frame.filter(|_| host.input.band.is_some()) else {
        return;
    };
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
}

/// The painted tiles' slot and art rectangles, so a press between them reaches the band and a press on one does not.
pub(crate) fn grid_hit_boxes(host: &PaneHost, bounds: Bounds<Pixels>) -> Option<super::HitBoxes> {
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
}

/// Only a painted Grid caption can arm a slow rename tap.
pub(crate) fn grid_on_name(host: &PaneHost, row: usize, point: gpui::Point<Pixels>) -> bool {
    host.grid.as_ref().is_some_and(|grid| {
        let state = grid.state.borrow();
        let at = point - state.bounds.origin;
        state
            .drawn_row(row)
            .is_some_and(|tile| tile.painted.contains(f32::from(at.x), f32::from(at.y)))
    })
}

impl Shell {
    /// An empty-ground left press between Grid tiles: not on a tile, not in the lane, no carry, the pane idle and landed.
    pub(crate) fn band_press(
        &mut self,
        side: usize,
        frame: Frame,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.input_blocked(cx) {
            return false;
        }
        let carrying = cx
            .try_global::<crate::g2::dg1::carry::Carrying>()
            .is_some_and(|carrying| carrying.carry.active());
        let Some(host) = self.panes[side].as_mut() else {
            return false;
        };
        if event.button != MouseButton::Left
            || carrying
            || host.stopped
            || host.pane.busy()
            || host.grid.as_ref().is_some_and(|grid| !grid.band_ready())
            || host.input.hit_in(frame, event.position).is_some()
            || frame.in_lane(event.position)
        {
            return false;
        }
        host.input.frame = Some(frame);
        host.pane.scroll_extent = Some(frame.content);
        host.pane.scroll_to(frame.scroll);
        if let Some(grid) = host.grid.as_mut() {
            grid.view.land_glide();
        }
        true
    }
}
