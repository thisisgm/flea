//! A still pointer re-hovers the track row a scroll or a released drag brought under it, as Qt does.
use crate::{columns::geometry::Slot, window::pane::PaneHost};
use gpui::{Pixels, Point, Window};
use std::rc::Rc;

/// Read the hover again from where the pointer rests, and ask one catch-up frame while the paint lags behind it.
pub(super) fn rehover(host: &mut PaneHost, blocked: bool, window: &mut Window) {
    let Some(at) = host
        .input
        .resting()
        .filter(|_| !blocked && !host.pane.busy())
    else {
        return;
    };
    let hover = crate::window::columns::hover_at(host, at);
    let lag = painted_lag(host, at);
    let Some(columns) = host.columns.as_mut() else {
        return;
    };
    columns.hover = hover;
    // One catch-up frame per state the paint lags behind, so a track that never repaints cannot spin frames.
    if lag.is_some() && lag != columns.painters.rest_retried {
        window.request_animation_frame();
    }
    columns.painters.rest_retried = lag;
}

/// The tracks and scroll under the pointer while that track's last paint was for another state, so its hover lags a frame.
fn painted_lag(host: &PaneHost, at: Point<Pixels>) -> Option<(usize, u32)> {
    let columns = host.columns.as_ref()?;
    let tracks = columns.area.tracks();
    tracks.iter().enumerate().find_map(|(index, view)| {
        let paint = columns.painters.get(view.slot)?.borrow();
        let drawn = paint
            .drawn
            .as_ref()
            .filter(|drawn| drawn.bounds.contains(&at))?;
        let scroll = if view.slot == Slot::Active {
            host.pane.scroll_y
        } else {
            columns.area.scroll(view.slot)
        };
        let scroll = columns.scroll[view.slot.index()].input.drawn(scroll);
        (!Rc::ptr_eq(&drawn.tracks, tracks) || drawn.at != index || drawn.scroll != scroll)
            .then(|| (Rc::as_ptr(tracks) as *const () as usize, scroll.to_bits()))
    })
}
