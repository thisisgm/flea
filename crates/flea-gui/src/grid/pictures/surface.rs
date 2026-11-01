//! Which surface draws a pane's rows, the Grid, the List or the active Columns track, and the rows its last frame drew.
use crate::columns::{geometry::Slot as Track, TrackRows};
use crate::list::pane::ROW_HEIGHT;
use crate::{state::Shell, window::pane::PaneHost, window::startup::View};
use std::rc::Rc;

// The surface that draws a pane's rows this frame, whose viewport its pictures follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Surface {
    Grid,
    List,
    Columns,
}

// The rows one surface draws: a listing span, and for a List filter its shown rows with the drawn view positions, end exclusive.
pub(crate) struct Want {
    pub span: (usize, usize),
    pub only: Option<(Rc<[usize]>, usize, usize)>,
}

impl Want {
    // The listing rows a settle may ask: the span, or a filter's drawn rows only, Filter.cut.
    pub fn rows(&self) -> Vec<usize> {
        match &self.only {
            Some((shown, first, end)) => shown[*first..*end].to_vec(),
            None => (self.span.0..=self.span.1).collect(),
        }
    }
}

/// The rows the surface drew in its last frame at the pane's current scroll; None while it has not drawn.
pub(super) fn drawn_rows(host: &PaneHost, surface: Surface) -> Option<Want> {
    match surface {
        Surface::Grid => {
            let grid = host.grid.as_ref()?;
            let span = grid
                .view
                .wanted_thumbnails(&host.pane, crate::grid::Shown::All(host.pane.held.total))?;
            Some(Want {
                span: (*span.start(), *span.end()),
                only: None,
            })
        }
        Surface::List => list_rows(host),
        Surface::Columns => columns_rows(host),
    }
}

// Before a List or Columns frame has drawn, the pane's own viewport stands in so the settle starts; it asks only drawn rows.
pub(super) fn estimated_rows(host: &PaneHost) -> Want {
    let first = (host.pane.scroll_y.max(0.) / ROW_HEIGHT).floor() as usize;
    Want {
        span: (first, first + host.pane.visible_rows() - 1),
        only: None,
    }
}

// ListElement's own row range over the frame it last drew, at the pane's current scroll; None until it has drawn.
fn list_rows(host: &PaneHost) -> Option<Want> {
    let list = host.list.borrow();
    let (height, row) = (
        f32::from(list.drawn.bounds.size.height),
        list.drawn.metrics.row,
    );
    if height <= 0. || row <= 0. {
        return None;
    }
    let scroll = host.pane.scroll_y;
    let first = (scroll / row).floor().max(0.) as usize;
    let shown = host.filter.shown.as_ref();
    let total = shown.map_or(host.pane.held.total, |rows| rows.len());
    let end = (((scroll + height) / row).ceil().max(0.) as usize).min(total);
    drawn_want(first, end, shown)
}

// The listing rows at drawn positions first..end: the positions themselves, or a filter's rows there (Filter.cut).
fn drawn_want(first: usize, end: usize, shown: Option<&Rc<[usize]>>) -> Option<Want> {
    match shown {
        None => (end > first).then(|| Want {
            span: (first, end - 1),
            only: None,
        }),
        // A filter that draws nothing still names its rows, so its settle cancels what waits.
        Some(rows) => {
            let first = first.min(end);
            let drawn = &rows[first..end];
            let span = (
                drawn.iter().min().copied().unwrap_or(0),
                drawn.iter().max().copied().unwrap_or(0),
            );
            Some(Want {
                span,
                only: Some((Rc::clone(rows), first, end)),
            })
        }
    }
}

// TrackElement's row range for the active track over the frame it last drew, at the pane's current scroll.
fn columns_rows(host: &PaneHost) -> Option<Want> {
    let columns = host.columns.as_ref()?;
    let paint = columns.painters.get(Track::Active)?.borrow();
    let drawn = paint.drawn.as_ref()?;
    if !matches!(drawn.view().rows, TrackRows::Listing) {
        return None;
    }
    let height = f32::from(drawn.bounds.size.height);
    let scroll = host.pane.scroll_y.max(0.);
    let first = (scroll / ROW_HEIGHT).floor() as usize;
    // The track draws a standing filter's rows, so its pictures are asked for those rows, as the List's are (SW-14).
    let shown = drawn.shown.as_ref();
    let total = shown.map_or(host.pane.held.total, |rows| rows.len());
    let end = (((scroll + height) / ROW_HEIGHT).ceil() as usize).min(total);
    drawn_want(first, end, shown)
}

impl Shell {
    // Which surface draws this pane's rows: the Grid and Columns on the first pane, the List there or on either Dual pane.
    pub(super) fn picture_surface(&self, side: usize) -> Option<Surface> {
        if self.slots.covers[side].is_some() {
            return None;
        }
        match self.shown_view() {
            View::Grid if side == 0 => Some(Surface::Grid),
            View::List if side == 0 => Some(Surface::List),
            View::Dual => Some(Surface::List),
            View::Columns if side == 0 => Some(Surface::Columns),
            _ => None,
        }
    }
}
