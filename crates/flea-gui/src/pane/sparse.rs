//! Filtered viewports fetch bounded windows and hold only the visible listing rows.
use super::{
    requests::{Requests, WindowRequest, BUFFER},
    Held, Pane,
};
use flea::protocol::RowFacts;
use std::rc::Rc;

impl Requests {
    pub(crate) fn wanted_sparse(&mut self, held: &Held, wanted: &[usize]) -> Option<WindowRequest> {
        if self.outstanding.is_none() && wanted.iter().all(|&row| held.row(row).is_some()) {
            return None;
        }
        let mut wanted = wanted.to_vec();
        wanted.sort_unstable();
        wanted.dedup();
        if self.outstanding.is_some() {
            if self.sparse.as_ref() != Some(&wanted) {
                self.sparse = Some(wanted);
            }
            return None;
        }
        if wanted.is_empty() {
            return None;
        }
        let missing = wanted.iter().position(|&row| held.row(row).is_none())?;
        let start = wanted[missing];
        // One window reaches as far as a dense window for this viewport would, so a sparse filter costs a few round trips, not one per tile.
        let reach = start.saturating_add(wanted.len().saturating_add(2 * BUFFER));
        let last = wanted[missing..]
            .iter()
            .take_while(|&&row| row < reach)
            .filter(|&&row| held.row(row).is_none())
            .last()
            .copied()
            .unwrap_or(start);
        let request = WindowRequest {
            start,
            count: last - start + 1,
            listing: held.listing,
        };
        let request = self.begin(request)?;
        self.sparse = Some(wanted);
        self.initial_first = None;
        Some(request)
    }
}

impl Pane {
    pub(crate) fn take_sparse_rows(
        &mut self,
        start: usize,
        rows: Vec<RowFacts>,
        listing: usize,
    ) -> Result<bool, String> {
        if self.busy() || !self.requests.answers(&self.held, start, listing) {
            return Ok(false);
        }
        let Some(request) = self.requests.outstanding else {
            return Ok(false);
        };
        if let Err(error) = Self::check_complete(start, rows.len(), self.held.total, request.count)
        {
            self.requests.cancel();
            self.requests.refused = Some(request);
            return Err(error);
        }
        let wanted = self.requests.sparse.take().unwrap_or_default();
        let Some(&first) = wanted.first() else {
            self.requests.cancel();
            return Ok(false);
        };
        let rows = wanted
            .iter()
            .filter_map(|&index| {
                let facts = index
                    .checked_sub(start)
                    .and_then(|at| rows.get(at))
                    .or_else(|| self.held.row(index))?;
                let mut facts = facts.clone();
                facts.listing_index = Some(index);
                Some(facts)
            })
            .collect();
        self.requests.cancel();
        self.held = Rc::new(Held {
            listing,
            total: self.held.total,
            start: first,
            rows,
        });
        Ok(true)
    }
}

impl Requests {
    pub(crate) fn keep_sparse_span(&mut self, held: &Held, first: usize, visible: usize) {
        if self.sparse.is_some() {
            self.sparse = Some((first..first.saturating_add(visible).min(held.total)).collect());
        }
    }
}
