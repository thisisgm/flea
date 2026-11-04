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
fn destination_span(pane: &Pane, height: f32) -> (usize, usize) {
    let row = pane.row_height();
    let max = (pane.held.total as f32 * row - height).max(0.);
    let mut scroll = pane.scroll_y.clamp(0., max);
    let top = pane.cursor as f32 * row;
    if top < scroll {
        scroll = top.clamp(0., max);
    } else if top + row > scroll + height {
        scroll = (top + row - height).clamp(0., max);
    }
    let first = (scroll / row).floor() as usize;
    let last = ((scroll + height) / row).ceil() as usize;
    (first, last.min(pane.held.total))
}

fn covers(pane: &Pane, (first, last): (usize, usize)) -> bool {
    first >= pane.held.start && last <= pane.held.start + pane.held.rows.len()
}

impl Shell {
    fn grid_destination_height(&self, next: View, window: &Window) -> f32 {
        self.seam.viewport().map_or_else(
            || {
                self.geometry_for(next, window)
                    .content_height(next, self.slots.strip_height)
            },
            |size| f32::from(size.height),
        )
    }

    pub(crate) fn defer_grid_view(&mut self, next: View, window: &Window) -> bool {
        if self.view != View::Grid {
            return false;
        }
        let height = self.grid_destination_height(next, window);
        let Some(host) = self.panes[self.focus_side].as_mut() else {
            return false;
        };
        let Some(grid) = host.grid.as_mut() else {
            return false;
        };
        if next == View::Grid
            || host.pane.state.kind != crate::pane::Kind::Ready
            || covers(&host.pane, destination_span(&host.pane, height))
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
        let height = self.grid_destination_height(next, window);
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
            if !covers(&host.pane, destination_span(&host.pane, height)) {
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
        let span = destination_span(&host.pane, height);
        if host.pane.state.kind != crate::pane::Kind::Ready || covers(&host.pane, span) {
            grid.next_view = None;
            self.apply_view(next, window, cx);
            return;
        }
        let failure = if let Some(backend) = &host.backend {
            host.pane
                .requests
                .wanted(&host.pane.held, span.0, span.1.saturating_sub(span.0))
                .and_then(|request| backend.window(request.start, request.count).err())
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
        let height = self.grid_destination_height(self.view, window);
        for host in self.panes.iter_mut().flatten() {
            if let Some(grid) = &mut host.grid {
                grid.park();
            }
            host.nav.viewport = None;
            host.nav.grid_landing = None;
            host.pane.set_height(height);
            host.pane.reveal_cursor();
        }
    }
}
