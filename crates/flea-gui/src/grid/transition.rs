//! A view switch commits only once the destination's complete viewport is held.
use super::FocusSnapshot;
use crate::{
    pane::{Held, Pane},
    state::Shell,
    window::startup::View,
};
use gpui::{Context, Window};
use std::rc::Rc;

pub(super) struct PendingView {
    next: View,
    pub held: Rc<Held>,
    pub focus: FocusSnapshot,
    // The last observed destination, which can be replaced repeatedly while the drawn frame stays held.
    destination_listing: usize,
}

// Match Pane::set_height followed by reveal_cursor, without changing the drawn Grid.
fn destination_scroll(pane: &Pane, height: f32, shown: Option<&[usize]>) -> f32 {
    let row = pane.row_height();
    let total = shown.map_or(pane.held.total, <[usize]>::len);
    let cursor = shown
        .and_then(|rows| rows.iter().position(|&row| row == pane.cursor))
        .unwrap_or(pane.cursor);
    let max = (total as f32 * row - height).max(0.);
    let mut scroll = pane.scroll_y.clamp(0., max);
    let top = cursor as f32 * row;
    if top < scroll {
        scroll = top.clamp(0., max);
    } else if top + row > scroll + height {
        scroll = (top + row - height).clamp(0., max);
    }
    scroll
}

fn destination_span(pane: &Pane, height: f32, shown: Option<&[usize]>) -> (usize, usize) {
    let row = pane.row_height();
    let total = shown.map_or(pane.held.total, <[usize]>::len);
    let scroll = destination_scroll(pane, height, shown);
    let first = (scroll / row).floor() as usize;
    let last = ((scroll + height) / row).ceil() as usize;
    (first, last.min(total))
}

fn prefetch_span(
    pane: &Pane,
    height: f32,
    landing: f32,
    shown: Option<&[usize]>,
) -> (usize, usize) {
    let Some(shown) = shown else {
        return destination_span(pane, height, None);
    };
    let row = pane.row_height();
    let scroll = destination_scroll(pane, landing, Some(shown));
    let grown = scroll.min((shown.len() as f32 * row - height).max(0.));
    let first = (scroll.min(grown) / row).floor() as usize;
    let end = ((scroll + landing).max(grown + height) / row).ceil() as usize;
    (first, end.min(shown.len()))
}

fn covers(pane: &Pane, (first, last): (usize, usize), shown: Option<&[usize]>) -> bool {
    match shown {
        Some(rows) => rows[first..last]
            .iter()
            .all(|&row| pane.held.row(row).is_some()),
        None => (first..last).all(|row| pane.held.row(row).is_some()),
    }
}

impl Shell {
    fn grid_destination_heights(&self, next: View, window: &Window) -> (f32, f32) {
        if let Some(size) = self.seam.viewport() {
            let height = f32::from(size.height);
            return (height, height);
        }
        let geometry = self.geometry_for(next, window);
        let height = geometry.content_height(next, self.slots.strip_height);
        let landing = self.panes[self.focus_side].as_ref().map_or(height, |host| {
            host.landing_height(
                next,
                geometry.content_height(View::List, self.slots.strip_height),
                height,
            )
        });
        (height, landing)
    }

    pub(crate) fn defer_grid_view(&mut self, next: View, window: &Window) -> bool {
        if self.view != View::Grid {
            return false;
        }
        let (height, landing) = self.grid_destination_heights(next, window);
        let Some(host) = self.panes[self.focus_side].as_mut() else {
            return false;
        };
        let Some(grid) = host.grid.as_mut() else {
            return false;
        };
        if next == View::Grid
            || host.pane.state.kind != crate::pane::Kind::Ready
            || covers(
                &host.pane,
                prefetch_span(&host.pane, height, landing, host.filter.shown.as_deref()),
                host.filter.shown.as_deref(),
            )
        {
            grid.next_view = None;
            return false;
        }
        if let Some(pending) = &mut grid.next_view {
            pending.next = next;
        } else {
            // A fresh mode change owns only refusals arriving after it was staged.
            host.pane.requests.refused = None;
            let state = grid.state.borrow();
            grid.next_view = Some(PendingView {
                next,
                destination_listing: host.pane.held.listing,
                held: Rc::clone(state.frame_held.as_ref().unwrap_or(&host.pane.held)),
                focus: FocusSnapshot {
                    cursor: state
                        .drawn()
                        .iter()
                        .find(|tile| tile.cursor)
                        .map_or(host.pane.cursor, |tile| tile.listing),
                    selected: state
                        .drawn()
                        .iter()
                        .filter(|tile| tile.selected)
                        .map(|tile| tile.listing)
                        .collect(),
                    ..Default::default()
                },
            });
        }
        true
    }

    pub(crate) fn finish_grid_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view != View::Grid {
            return;
        }
        let Some(next) = self.panes[self.focus_side]
            .as_ref()
            .and_then(|host| host.grid.as_ref())
            .and_then(|grid| grid.next_view.as_ref())
            .map(|pending| pending.next)
        else {
            return;
        };
        let (height, landing) = self.grid_destination_heights(next, window);
        let host = self.panes[self.focus_side]
            .as_mut()
            .expect("pending Grid pane");
        let grid = host.grid.as_mut().expect("pending Grid host");
        if host.pane.state.kind != crate::pane::Kind::Ready && !host.pane.busy() {
            grid.next_view = None;
            self.apply_view(next, window, cx);
            return;
        }
        // A listing out keeps the staged switch on the drawn Grid frame until its rows land.
        if host.pane.busy() {
            return;
        }
        let listing = host.pane.held.listing;
        let pending = grid.next_view.as_mut().expect("pending view");
        let superseded = pending.held.listing != listing;
        if pending.destination_listing != listing {
            // Each replacement retires only the previous destination's refusal, never its own.
            pending.destination_listing = listing;
            if grid.refused != Some(listing) {
                grid.refused = None;
                grid.refusal.clear();
            }
            if host
                .pane
                .requests
                .refused
                .is_some_and(|request| request.listing != listing)
            {
                host.pane.requests.refused = None;
            }
        }
        let refused = grid.refused == Some(listing)
            || (host.pane.requests.outstanding.is_none()
                && host
                    .pane
                    .requests
                    .refused
                    .is_some_and(|request| request.listing == host.pane.held.listing));
        if refused {
            if superseded {
                // The new listing cannot fill the destination, so it lands on its complete error surface.
                let failure = crate::backend::Failure {
                    key: "window.backend_report",
                    detail: std::mem::take(&mut grid.refusal),
                };
                grid.next_view = None;
                self.receive(
                    self.focus_side,
                    crate::backend::Event::Failed(failure),
                    window,
                    cx,
                );
                self.apply_view(next, window, cx);
                return;
            }
            grid.refused = Some(listing);
            host.pane.held = Rc::clone(&grid.next_view.as_ref().expect("pending view").held);
            grid.settle_refusal(&mut host.pane);
            if !covers(
                &host.pane,
                prefetch_span(&host.pane, height, landing, host.filter.shown.as_deref()),
                host.filter.shown.as_deref(),
            ) {
                // The held rows cannot draw the destination, so it lands on its complete error surface.
                let failure = crate::backend::Failure {
                    key: "window.backend_report",
                    detail: std::mem::take(&mut grid.refusal),
                };
                grid.next_view = None;
                self.receive(
                    self.focus_side,
                    crate::backend::Event::Failed(failure),
                    window,
                    cx,
                );
                self.apply_view(next, window, cx);
                return;
            }
        }
        let shown = host.filter.shown.as_deref();
        let span = prefetch_span(&host.pane, height, landing, shown);
        if host.pane.state.kind != crate::pane::Kind::Ready || covers(&host.pane, span, shown) {
            grid.next_view = None;
            self.apply_view(next, window, cx);
            return;
        }
        let failure = if let Some(backend) = &host.backend {
            let wanted = match shown {
                Some(rows) => host
                    .pane
                    .requests
                    .wanted_sparse(&host.pane.held, &rows[span.0..span.1]),
                None => host.pane.requests.wanted(
                    &host.pane.held,
                    span.0,
                    span.1.saturating_sub(span.0),
                ),
            };
            wanted.and_then(|request| backend.window(request.start, request.count).err())
        } else {
            Some(crate::backend::Failure {
                key: "window.backend_stopped",
                detail: String::new(),
            })
        };
        if let Some(failure) = failure {
            host.stopped = true;
            grid.next_view = None;
            self.receive(
                self.focus_side,
                crate::backend::Event::Failed(failure),
                window,
                cx,
            );
            self.apply_view(next, window, cx);
        }
    }

    pub(crate) fn leave_grid(&mut self, previous: View, window: &Window) {
        if previous != View::Grid || self.view == View::Grid {
            return;
        }
        let (height, landing) = self.grid_destination_heights(self.view, window);
        for host in self.panes.iter_mut().flatten() {
            let scroll = host
                .filter
                .shown
                .as_deref()
                .map(|rows| destination_scroll(&host.pane, landing, Some(rows)));
            if let Some(grid) = &mut host.grid {
                grid.park();
            }
            host.nav.viewport = None;
            host.nav.grid_landing = None;
            host.pane
                .set_height(if scroll.is_some() { landing } else { height });
            if let Some(scroll) = scroll {
                // Land on the filtered viewport whose rows were fetched, before an old extent can clamp it.
                host.pane.scroll_y = scroll;
            } else {
                host.pane.reveal_cursor();
            }
        }
    }
}

#[cfg(test)]
mod filtered_tests {
    use super::*;
    use flea::protocol::RowFacts;

    #[test]
    fn fx477_filtered_destination_span_tracks_the_cursor_view_position() {
        let mut pane = Pane::new("/files".into(), crate::pane::ROW_HEIGHT);
        pane.held = Rc::new(Held {
            listing: 2,
            total: 3000,
            rows: [60, 1500, 2880]
                .into_iter()
                .map(|index| RowFacts {
                    listing_index: Some(index),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        pane.cursor = 1500;
        let shown = [60, 1500, 2880];
        let span = destination_span(&pane, pane.row_height(), Some(&shown));
        assert_eq!(span, (1, 2));
        assert!(covers(&pane, span, Some(&shown)));
        Rc::get_mut(&mut pane.held)
            .unwrap()
            .rows
            .retain(|row| row.listing_index != Some(1500));
        assert!(!covers(&pane, span, Some(&shown)));
    }
}
