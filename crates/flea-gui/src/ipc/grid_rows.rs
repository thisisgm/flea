//! Mounted Grid readers expose only the tiles and theme roles recorded by its last frame.
use super::{
    grid::GridAdapter,
    rows::{centre_of, colour, rect_of},
    Adapter,
};
use crate::grid::{element::GridState, host::GridHost};
use flea::ipc::wire::Reply;

pub(crate) struct MountedGrid<'a> {
    pub grid: Option<&'a GridHost>,
    pub active: bool,
}

impl Adapter for MountedGrid<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function == "gridColumns" && !self.active {
            return Some(Reply::value("1"));
        }
        if function == "gridCaptionState" && !self.active {
            return Some(Reply::value("{}"));
        }
        if !self.active {
            return None;
        }
        // Active but not yet drawn: the row readers answer empty, never the parked List's.
        let Some(grid) = self.grid else {
            return unpainted(function);
        };
        let state = grid.state.borrow();
        GridAdapter {
            view: &grid.view,
            state: &state,
        }
        .call(function, arguments)
        .or_else(|| {
            grid_rows(&state, grid.view.metrics.file_row, function, arguments).map(Reply::value)
        })
    }
}

fn unpainted(function: &str) -> Option<Reply> {
    let value = match function {
        "gridColumns" | "visibleRows" => "1",
        "rowHovered" | "rowThumbReady" => "false",
        "gridCaptionState" => "{}",
        "listingBackgroundCentre"
        | "visibleRowName"
        | "rowRect"
        | "rowCentre"
        | "rowNameRect"
        | "rowNameCentre"
        | "rowNameText"
        | "rowThumbRect"
        | "rowNameColor" => "",
        _ => return None,
    };
    Some(Reply::value(value))
}

// ui/Pane.qml:281 answers max(1, ceil(list.height / Theme.fileRowHeight)).
fn visible_rows(height: f32, file_row: f32) -> usize {
    ((height / file_row).ceil() as usize).max(1)
}

fn grid_rows(
    state: &GridState,
    file_row: f32,
    function: &str,
    arguments: &[String],
) -> Option<String> {
    let bounds = state.bounds;
    let (x, y) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let window_rect = |r: crate::grid::geometry::Rect| rect_of(x + r.x, y + r.y, r.width, r.height);
    let centre = |r: crate::grid::geometry::Rect| centre_of(x + r.x, y + r.y, r.width, r.height);
    let value = match function {
        "listingBackgroundCentre" => super::rows::centre_of_bounds(bounds),
        "listAreaRect" => rect_of(
            x,
            y,
            f32::from(bounds.size.width),
            f32::from(bounds.size.height),
        ),
        "headerTop" => y.round().to_string(),
        "listContentY" | "viewContentY" => state.scroll.round().to_string(),
        "viewEndY" => state.layout?.max_scroll().round().to_string(),
        "visibleRows" => visible_rows(f32::from(bounds.size.height), file_row).to_string(),
        "drawnCount" => state.layout?.total.to_string(),
        "visibleRowName" => {
            let index = arguments.first()?.parse::<usize>().ok();
            index
                .and_then(|index| state.drawn_row(index))
                .and_then(|d| d.name())
                .unwrap_or("")
                .into()
        }
        _ => {
            let row = arguments
                .first()?
                .parse::<usize>()
                .ok()
                .and_then(|index| state.drawn_row(index));
            match function {
                "rowRect" => row.map(|d| window_rect(d.cell)).unwrap_or_default(),
                "rowCentre" => row
                    .filter(|d| d.name().is_some())
                    .map(|d| centre(d.cell))
                    .unwrap_or_default(),
                "rowNameRect" => row
                    .filter(|d| !d.editing)
                    .map(|d| window_rect(d.caption))
                    .unwrap_or_default(),
                "rowNameCentre" => row
                    .filter(|d| !d.editing)
                    .map(|d| centre(d.painted))
                    .unwrap_or_default(),
                "rowNameText" => row.and_then(|d| d.caption_text()).unwrap_or("").into(),
                "rowHovered" => row.is_some_and(|d| d.hovered).to_string(),
                "rowThumbRect" => row.map(|d| window_rect(d.slot)).unwrap_or_default(),
                "rowThumbReady" => row.is_some_and(|d| d.decoded.is_some()).to_string(),
                "rowNameColor" => row
                    .and(state.ink)
                    .map(|ink| colour(ink.foreground))
                    .unwrap_or_default(),
                _ => return None,
            }
        }
    };
    Some(value)
}
