use crate::{state::Shell, window::startup::View};
use gpui::{Context, MouseButton, Pixels, Point, Window};
use std::rc::Rc;

impl Shell {
    pub(crate) fn middle_tab_pointer(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Middle || self.input_blocked(cx) {
            return;
        }
        for side in 0..2 {
            if self.middle_tab_row(side, event.position, window, cx) {
                cx.stop_propagation();
                return;
            }
        }
    }

    pub(crate) fn middle_tab_row(
        &mut self,
        side: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(host) = self.panes.get(side).and_then(Option::as_ref) else {
            return false;
        };
        // An empty active folder can still have painted parent-folder targets in Columns.
        if host.pane.busy()
            || host.stopped
            || !matches!(
                host.pane.state.kind,
                crate::pane::Kind::Ready | crate::pane::Kind::Empty
            )
        {
            return false;
        }
        let target = match host.drawn_view {
            Some(View::List | View::Dual) => {
                let list = host.list.borrow();
                let drawn = &list.drawn;
                if !host.drawn_list
                    || drawn.frames == 0
                    || drawn.listing != host.pane.held.listing
                    || !drawn.mask.contains(&position)
                {
                    return false;
                }
                let local = f32::from(position.y - drawn.bounds.origin.y) + drawn.scroll_y;
                let index = (local / drawn.metrics.row).floor() as usize;
                host.pane
                    .held
                    .row(index)
                    .filter(|row| row.dir)
                    .map(|row| crate::navigation::history::join(host.pane.path(), &row.name))
            }
            Some(View::Grid) => {
                let Some(grid) = &host.grid else { return false };
                let drawn = grid.state.borrow();
                if !drawn.bounds.contains(&position)
                    || drawn
                        .frame_held
                        .as_ref()
                        .is_none_or(|held| !Rc::ptr_eq(held, &host.pane.held))
                {
                    return false;
                }
                let at = drawn.tile_at(
                    f32::from(position.x - drawn.bounds.origin.x),
                    f32::from(position.y - drawn.bounds.origin.y),
                );
                at.and_then(|at| drawn.drawn().iter().find(|tile| tile.view == at))
                    .and_then(|tile| host.pane.held.row(tile.listing))
                    .filter(|row| row.dir)
                    .map(|row| crate::navigation::history::join(host.pane.path(), &row.name))
            }
            Some(View::Columns) => {
                let Some(columns) = &host.columns else {
                    return false;
                };
                let tracks = columns.area.tracks();
                tracks.iter().enumerate().find_map(|(at, view)| {
                    let paint = columns.painters.get(view.slot)?.borrow();
                    let drawn = paint.drawn.as_ref()?;
                    if !view.enabled
                        || !Rc::ptr_eq(&drawn.tracks, tracks)
                        || drawn.at != at
                        || !drawn.bounds.contains(&position)
                    {
                        return None;
                    }
                    let index = ((f32::from(position.y - drawn.bounds.origin.y) + drawn.scroll)
                        / drawn.row_height)
                        .floor() as usize;
                    let name = match &drawn.view().rows {
                        crate::columns::TrackRows::Peek(rows) => rows
                            .get(index)
                            .filter(|row| row.dir)
                            .map(|row| row.name.as_str()),
                        crate::columns::TrackRows::Listing => drawn
                            .held
                            .as_ref()
                            .filter(|held| Rc::ptr_eq(held, &host.pane.held))
                            .and_then(|held| held.row(index))
                            .filter(|row| row.dir)
                            .map(|row| row.name.as_str()),
                        _ => None,
                    }?;
                    Some(crate::navigation::history::join(&view.path, name))
                })
            }
            None => None,
        };
        if let Some(target) = target {
            self.open_tab(side, target, window, cx);
            true
        } else {
            false
        }
    }
}
