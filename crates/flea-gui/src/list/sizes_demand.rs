//! Reconcile pending walks with the active surface's bounded, painted directory rows.
use super::{State, CAP};
use crate::{
    list::columns::Column, navigation::sort::wants_sizes, pane::requests::RowAction,
    window::pane::PaneHost,
};

pub(super) fn visible(host: &PaneHost, columns: bool) -> Vec<usize> {
    let class = host.sizes.class.as_deref();
    if !wants_sizes(
        if columns { "columns" } else { "list" },
        &[],
        class.unwrap_or_default(),
        class.is_some(),
    ) {
        return Vec::new();
    }
    let held = &host.pane.held;
    let span = if columns {
        let Some(columns) = &host.columns else {
            return Vec::new();
        };
        let Some(painter) = columns.painters.get(crate::columns::geometry::Slot::Active) else {
            return Vec::new();
        };
        let painter = painter.borrow();
        let Some(drawn) = &painter.drawn else {
            return Vec::new();
        };
        if drawn
            .held
            .as_ref()
            .is_none_or(|drawn| drawn.listing != held.listing)
        {
            return Vec::new();
        }
        drawn.grow.span(
            host.pane.scroll_y,
            drawn.bounds.size.height.into(),
            drawn.row_height,
            host.filter.shown_total(held.total),
        )
    } else {
        let list = host.list.borrow();
        if !host.drawn_list
            || list.drawn.listing != held.listing
            || !list.drawn.layout.shown(Column::Size)
        {
            return Vec::new();
        }
        let Some(frame) = host
            .input
            .prepared_frame()
            .filter(|frame| frame.listing == held.listing)
        else {
            return Vec::new();
        };
        frame.grow.span(
            host.pane.scroll_y,
            frame.viewport(),
            frame.row,
            host.filter.shown_total(held.total),
        )
    };
    // Share the painter's span and filtered position map, using this render's geometry.
    (span.0..span.1)
        .take(CAP)
        .map(|position| {
            host.filter
                .shown
                .as_ref()
                .map_or(position, |rows| rows[position])
        })
        .filter(|row| held.row(*row).is_some_and(|row| row.dir))
        .collect()
}

impl State {
    pub(crate) fn cancel(&mut self, backend: &crate::backend::Backend) {
        self.pause(backend);
        self.forget();
    }

    pub(super) fn forget(&mut self) {
        self.task = None;
        self.key = None;
        self.waiting.clear();
        self.paint_dirty |= self.sizes.clear_previous();
        self.sizes.cancelled();
    }

    // An in-place read suspends walks, retaining display bytes for its next listing.
    pub(super) fn pause(&mut self, backend: &crate::backend::Backend) {
        self.task = None;
        self.key = None;
        if !self.waiting.is_empty() {
            if backend.list_details(r#"{"c":"dirsizecancel"}"#).is_ok() {
                self.cancels += 1;
            }
            self.waiting.clear();
            self.sizes.pause();
        }
    }

    pub(super) fn retain_visible(
        &mut self,
        backend: &crate::backend::Backend,
        visible: &[usize],
    ) -> Result<(), crate::backend::Failure> {
        let dropped: Vec<_> = self
            .waiting
            .iter()
            .copied()
            .filter(|row| !visible.contains(row))
            .collect();
        if dropped.is_empty() {
            return Ok(());
        }
        if dropped.len() == self.waiting.len() {
            self.cancel(backend);
            return Ok(());
        }
        backend.rows(
            "dirsizecancel",
            RowAction {
                listing: self.held.listing,
                rows: dropped.clone(),
            },
        )?;
        self.cancels += 1;
        self.waiting.retain(|row| !dropped.contains(row));
        self.paint_dirty |= self.sizes.cancel_rows(&dropped);
        Ok(())
    }
}
