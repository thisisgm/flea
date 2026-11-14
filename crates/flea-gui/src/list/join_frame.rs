//! The one pass that prepares every list's header, body area, viewport and row states for a frame.
use super::{Metrics, RowStates};
use crate::{state::Shell, window::startup::View};
use gpui::{AnyElement, Context, IntoElement, Window};

/// What one frame mounts for the lists, prepared in one pass so the shared render loop only places it.
pub(crate) struct Lists {
    pub headers: [Option<(AnyElement, f32)>; 2],
    pub areas: [Option<AnyElement>; 2],
    pub states: [Option<RowStates>; 2],
    #[cfg(test)]
    pub prepared: [Option<crate::input::Frame>; 2],
}

impl Shell {
    /// Headers, body areas, the clamped viewport, scene marks and row states, in the order the settle needs.
    pub(crate) fn prepare_lists(
        &mut self,
        heights: [f32; 2],
        chrome: Option<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Lists {
        let left = self
            .search_header(0, window, cx)
            .or_else(|| self.list_header(0, cx));
        let right = self
            .search_header(1, window, cx)
            .or_else(|| self.list_header(1, cx));
        let headers = [left, right];
        let areas = [0, 1].map(|side| {
            self.panes[side]
                .as_ref()
                .map(|_| self.list_area(side).into_any_element())
        });
        let blocked = self.input_blocked(cx);
        let geometry = self.geometry(window);
        let view = self.shown_view();
        let viewport = self.seam.viewport();
        // One refusal band and one Grow per side for this render, so every list frame and the rename readers agree with the paint.
        let bands = [0, 1].map(|side| self.rename_band(side, window));
        let previous_bands = self.bands;
        self.bands = bands;
        let previous_grows = self.grows;
        self.grows = [0, 1].map(|side| self.frame_grow(side, bands[side]));
        let grows = self.grows;
        #[cfg(test)]
        let mut prepared = [None; 2];
        // Clamp the viewport before scene reveals and before the settle captures its scroll.
        for (side, host) in self.panes.iter_mut().enumerate() {
            let Some(host) = host else { continue };
            if (side == 1 && self.view != View::Dual) || self.slots.covers[side].is_some() {
                host.input.cancel(&mut host.selected, &mut host.pane.cursor);
                continue;
            }
            if blocked || host.pane.busy() {
                host.input.cancel(&mut host.selected, &mut host.pane.cursor);
                host.input.press = None;
                if blocked || !host.pane.swap.retains_rows() {
                    host.input.hovered = None;
                }
            }
            let now = cx.background_executor().now();
            // Resolve shared font metrics during frame preparation, never inside an observer.
            host.list.borrow_mut().measure(window);
            let header = Metrics::for_typeface(&host.list.borrow().typeface).header;
            let height = heights[side] + chrome.map_or(0., |chrome| chrome - header);
            // Clamp against the current header before capturing this frame's scroll.
            if matches!(view, View::List | View::Dual) && host.pane.scroll_extent.is_some() {
                let metrics = host.list.borrow().file_metrics();
                let total = host.filter.shown_total(host.pane.held.total);
                let extent = grows[side].content(total, metrics.row) + metrics.header;
                host.pane.scroll_extent = Some(extent);
            }
            host.pane.set_height(height);
            if matches!(view, View::List | View::Dual) {
                let metrics = host.list.borrow().file_metrics();
                let bounds = viewport.map_or_else(
                    || {
                        let rows = geometry.rows(
                            side,
                            self.view == View::Dual,
                            self.slots.strip_height_for(side),
                        );
                        gpui::Bounds::new(
                            gpui::point(gpui::px(rows.x), gpui::px(rows.y)),
                            gpui::size(gpui::px(rows.width), gpui::px(height)),
                        )
                    },
                    |viewport| {
                        gpui::Bounds::new(
                            gpui::point(gpui::px(0.), gpui::px(0.)),
                            gpui::size(viewport.width, gpui::px(height)),
                        )
                    },
                );
                let mut frame =
                    crate::input::Frame::list(bounds, &metrics, &host.pane, grows[side]);
                if grows[side].band > 0. && grows[side] != previous_grows[side] {
                    frame.content = grows[side]
                        .content(host.filter.shown_total(host.pane.held.total), metrics.row)
                        + metrics.header;
                    host.pane.scroll_extent = Some(frame.content);
                    host.pane
                        .scroll_to(crate::input::scroll::reveal(frame, grows[side].at, 0));
                    frame.scroll = host.pane.scroll_y;
                }
                // Invalidate old geometry before an edge step can move the current viewport.
                host.input
                    .prepare(frame, &mut host.selected, &mut host.pane.cursor);
                host.input.advance(now, &mut host.pane, &mut host.selected);
                host.input
                    .step_motion(now, &mut host.pane, cx.reduce_motion());
                frame.scroll = host.pane.scroll_y;
                host.input
                    .prepare(frame, &mut host.selected, &mut host.pane.cursor);
                // Grid and Columns re-read a resting pointer through their own hover, never this list frame.
                if matches!(view, View::List | View::Dual) && !blocked && !host.pane.busy() {
                    host.input.rehover(host.pane.scroll_y);
                }
                #[cfg(test)]
                {
                    prepared[side] = Some(frame);
                }
            } else {
                crate::input::views::prepare(host, view, now);
                host.input
                    .step_motion(now, &mut host.pane, cx.reduce_motion());
            }
            if host.input.animating(now, cx.reduce_motion()) {
                window.request_animation_frame();
            }
            if view == View::Grid
                && host
                    .grid
                    .as_mut()
                    .is_some_and(|grid| grid.view.step_glide(now))
            {
                window.request_animation_frame();
            }
        }
        if view == View::Columns && grows != previous_grows {
            self.reveal_rename_column(previous_grows);
        }
        if view == View::Grid && bands != previous_bands {
            self.reveal_rename_grid(previous_bands);
        }
        if let (Some((cursor, selection)), Some(host)) =
            (self.visual_marks(), self.panes[self.focus_side].as_mut())
        {
            crate::visual::apply_marks(host, cursor, &selection);
        }
        let states = [0, 1].map(|side| {
            self.panes[side]
                .as_ref()
                .map(|_| self.list_states(side, window, cx))
        });
        Lists {
            headers,
            areas,
            states,
            #[cfg(test)]
            prepared,
        }
    }

    /// Visible rows are requested before background filesystem details on the same backend pipe.
    pub(crate) fn settle_list_sizes(&mut self, window: &Window, cx: &mut Context<Self>) {
        for side in 0..2 {
            self.sync_list_sizes(side, window, cx);
        }
    }
}
