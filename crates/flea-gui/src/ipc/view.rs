//! Readers use the bounded snapshot and geometry the mounted view's last frame drew.
use super::{
    rows::{describe_row, RowsAdapter},
    Adapter,
};
use crate::columns::{geometry::Slot, TrackRows};
use crate::list::{pane::Held, ListState, RowStates};
use crate::window::{pane::PaneHost, startup::View};
use flea::ipc::wire::Reply;

#[cfg(test)]
#[path = "view_every_tests.rs"]
mod view_every_tests;

pub struct ViewAdapter<'a> {
    pub list: &'a ListState,
    /// The view the window drew for this pane.
    pub mode: Option<View>,
    /// The owner is drawing the List rather than an empty, loading or failed pane surface.
    pub shown: bool,
}

pub(crate) struct MountedView<'a> {
    view: ViewAdapter<'a>,
    host: &'a PaneHost,
}

impl<'a> ViewAdapter<'a> {
    pub(crate) fn mounted(self, host: &'a PaneHost) -> MountedView<'a> {
        MountedView { view: self, host }
    }
}

/// Ipc.qml readers that go through pane.itemFor(i), the List delegate at view position i, not at listing row i.
const VIEW_POSITION_READERS: [&str; 13] = [
    "rowNamePx",
    "rowClipMark",
    "rowNameColor",
    "rowCellColor",
    "rowDateColor",
    "rowCellOverflow",
    "rowSizeText",
    "columnSet",
    "rowIcon",
    "rowIconStatus",
    "rowNameText",
    "rowGlyph",
    "rowAt",
];

// Filter.js at(): under a standing filter view position i draws listing row shown[i]; past the matches it is no row.
pub(crate) fn listing_row(function: &str, at: usize, filter: Option<&[usize]>) -> usize {
    match filter {
        Some(shown) if VIEW_POSITION_READERS.contains(&function) => {
            shown.get(at).copied().unwrap_or(usize::MAX)
        }
        _ => at,
    }
}

impl MountedView<'_> {
    fn rows(&self, function: &str, index: usize, held: Option<&Held>, visible: bool) -> Reply {
        let row = held.filter(|_| visible).and_then(|held| held.row(index));
        let value = match function {
            "drawnCount" => held.map_or(0, |held| held.total).to_string(),
            "visibleRowName" => row.map(|row| row.name.clone()).unwrap_or_default(),
            "rowAt" => row.map_or_else(
                || "loading".into(),
                |row| describe_row(row, &self.view.list.view.words),
            ),
            _ => unreachable!(),
        };
        Reply::value(value)
    }
}

impl Adapter for MountedView<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function == "dirSizeRequests" {
            return Some(Reply::value(self.host.sizes.requests.to_string()));
        }
        // GPUI-only: v0.3.12 counted only the asks, so a scroll's cancellation could not be read.
        if function == "dirSizeCancels" {
            return Some(Reply::value(self.host.sizes.cancels.to_string()));
        }
        // GPUI-only: tests/ui-dirsortstale.sh adds this reader to a copied v0.3.12 Ipc.qml as JSON.stringify(pane.dirSizeState).
        if function == "dirSizeState" {
            return Some(Reply::value(self.host.sizes.dir_sizes().json()));
        }
        if function == "thumbRequests" {
            let sent = self
                .host
                .backend
                .as_ref()
                .map_or(0, crate::backend::Backend::thumb_requests);
            return Some(Reply::value(sent.to_string()));
        }
        // ui/Ipc.qml filterStamps: the drawn view and the wall-clock ms of the last query change and its narrowing.
        if function == "filterStamps" {
            let view = match self.view.mode {
                Some(View::List | View::Dual) => "list",
                Some(View::Grid) => "grid",
                Some(View::Columns) => "columns",
                None => "",
            };
            let filter = &self.host.filter;
            return Some(Reply::value(format!(
                "{{\"view\":\"{view}\",\"keyAt\":{},\"narrowedAt\":{}}}",
                filter.key_at, filter.narrowed_at
            )));
        }
        if !matches!(function, "rowAt" | "visibleRowName" | "drawnCount")
            || matches!(self.view.mode, Some(View::List | View::Dual))
        {
            return self.view.call(function, arguments);
        }
        let index = arguments
            .first()
            .and_then(|value| value.parse().ok())
            .unwrap_or(usize::MAX);
        match self.view.mode {
            Some(View::Columns) => {
                let painter = self
                    .host
                    .columns
                    .as_ref()
                    .and_then(|columns| columns.painters.get(Slot::Active));
                let Some(painter) = painter else {
                    return Some(self.rows(function, index, None, false));
                };
                let paint = painter.borrow();
                let drawn = paint
                    .drawn
                    .as_ref()
                    .filter(|drawn| matches!(drawn.view().rows, TrackRows::Listing));
                let held = drawn.and_then(|drawn| drawn.held.as_deref());
                // Ipc.qml drawnCount: the active column's count, narrowed by a standing filter.
                if function == "drawnCount" {
                    let shown = drawn
                        .and_then(|drawn| drawn.shown.as_deref())
                        .map(<[usize]>::len);
                    let count = held.map_or(0, |held| shown.unwrap_or(held.total));
                    return Some(Reply::value(count.to_string()));
                }
                Some(self.rows(function, index, held, paint.prepared(index).is_some()))
            }
            Some(View::Grid) => {
                let Some(grid) = &self.host.grid else {
                    return Some(self.rows(function, index, None, false));
                };
                let state = grid.state.borrow();
                Some(self.rows(
                    function,
                    index,
                    state.frame_held.as_deref(),
                    state.drawn_row(index).and_then(|row| row.name()).is_some(),
                ))
            }
            None => Some(self.rows(function, index, None, false)),
            Some(View::List | View::Dual) => unreachable!(),
        }
    }
}

impl Adapter for ViewAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let list = self.list;
        let drawn = &list.drawn;
        let mounted = matches!(self.mode, Some(View::List | View::Dual));
        if !mounted && !matches!(function, "viewMode" | "rowAt") {
            return None;
        }
        let ready = mounted && self.shown && drawn.frames > 0;
        let value = match function {
            "viewMode" => match self.mode {
                Some(View::List | View::Dual) => "list",
                Some(View::Grid) => "grid",
                Some(View::Columns) => "columns",
                None => "",
            }
            .to_owned(),
            // Ipc.qml drawnCount is listArea.count, so a standing filter answers its narrowed rows.
            "drawnCount" => if ready { drawn.count } else { 0 }.to_string(),
            "visibleRows" => {
                if drawn.frames == 0 {
                    return None;
                }
                ((f32::from(drawn.bounds.size.height) / drawn.metrics.row)
                    .ceil()
                    .max(1.) as usize)
                    .to_string()
            }
            "viewEndY" => {
                if drawn.frames == 0 {
                    return None;
                }
                let count = if ready { drawn.count } else { 0 };
                let end = drawn.grow.content(count, drawn.metrics.row) + drawn.metrics.header
                    - f32::from(drawn.bounds.size.height);
                ((end + 0.5).floor() as i64).to_string()
            }
            "visibleRowName" => {
                let index = arguments.first()?.parse::<usize>().ok();
                index
                    .filter(|_| ready)
                    .filter(|&index| {
                        list.visible()
                            .iter()
                            .any(|row| row.index == index && row.prepared.is_some())
                    })
                    .and_then(|index| list.drawn_rows.row(index))
                    .map(|row| row.name.clone())
                    .unwrap_or_default()
            }
            "rowAt" => {
                if !ready {
                    return Some(Reply::value("loading"));
                }
                return RowsAdapter {
                    list,
                    held: &list.drawn_rows,
                    states: &RowStates::default(),
                }
                .call(function, arguments);
            }
            _ => return None,
        };
        Some(Reply::value(value))
    }
}
