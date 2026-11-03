//! A complete answer publishes once; an in-place update retains the viewport.
use super::*;

impl Pane {
    /// A retained viewport needs its bounded replacement before any new numbering is published.
    pub(crate) fn retained_window(&self) -> Option<(usize, usize)> {
        if !self.swap.retains_rows() {
            return None;
        }
        let pending = self.swap.pending.as_ref()?;
        if pending.options.independent_viewport {
            return None;
        }
        let held = pending.ready.as_ref()?;
        let cursor = self.landing_cursor(pending, held);
        let scroll = (self.scroll_y + (cursor as f32 - self.cursor as f32) * self.row_height)
            .clamp(
                0.,
                (held.total as f32 * self.row_height - self.height).max(0.),
            );
        let start = (scroll / self.row_height).floor() as usize;
        let end = ((scroll + self.height) / self.row_height).ceil() as usize;
        let end = end.min(held.total);
        (held.total > 0 && (start < held.start || end > held.start + held.rows.len()))
            .then_some((start, cursor))
    }

    pub(crate) fn commit(&mut self, now: u64) -> bool {
        let Some(mut pending) = self.swap.pending.take() else {
            return false;
        };
        let Some(held) = pending.ready.take() else {
            self.swap.pending = Some(pending);
            return false;
        };
        let held = pending.following.take().unwrap_or(held);
        let retained = pending.options.in_place && self.swap.holding;
        let cursor = pending
            .retained_cursor
            .unwrap_or_else(|| self.landing_cursor(&pending, &held));
        self.swap.retained_listing = retained.then_some(self.held.listing);
        if retained {
            self.scroll_y = cursor as f32 * self.row_height
                - (self.cursor as f32 * self.row_height - self.scroll_y);
        } else {
            self.scroll_y = held.start as f32 * self.row_height;
        }
        self.cursor = cursor;
        self.scroll_extent = None;
        self.path = pending.path;
        self.named = true;
        self.swap.listing_first = pending.first;
        // Wire numbering still changes so stale row actions cannot address newly inserted files.
        self.greatest_listing = held.listing;
        self.state = State {
            kind: if held.total == 0 {
                Kind::Empty
            } else {
                Kind::Ready
            },
            target: self.path.clone(),
            query: pending.options.kept_query,
            search: pending.options.search,
            ..State::default()
        };
        self.held = Rc::new(held);
        self.scroll_by(0.);
        self.requests.initial_first = Some((self.scroll_y / self.row_height).floor() as usize);
        self.swap.counters.commits += 1;
        self.swap.stamp_rows(now);
        self.swap.end(now, "landed");
        true
    }
}
