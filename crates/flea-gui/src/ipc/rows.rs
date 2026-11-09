//! Read-only row and header adapters: what the last List and header frames drew, in ui/Ipc.qml's shapes.
use super::Adapter;
use crate::{
    list::{
        columns::{Cell, Column, Layout},
        header::HeaderState,
        pane::Held,
        ListState, Metrics, Palette, RowState, RowStates, Shown,
    },
    row::{
        self, format,
        marks::{ClipMark, Mark},
    },
};
use flea::ipc::wire::Reply;
use gpui::{Bounds, Hsla, Pixels};
#[cfg(test)]
#[path = "row_centres_tests.rs"]
mod centres_tests;
#[path = "rows_format.rs"]
mod geometry_text;
use geometry_text::rect_of_bounds;
pub(crate) use geometry_text::round;
pub(crate) use geometry_text::{centre_inside, centre_of_bounds};
pub use geometry_text::{centre_of, colour, rect_of};

/// One pane's List, its held rows and the states its owners hold.
pub struct RowsAdapter<'a> {
    pub list: &'a ListState,
    pub held: &'a Held,
    pub states: &'a RowStates,
}

// A drawn row's box in window pixels, from the frame the list last painted.
fn drawn_row_box(list: &ListState, index: usize) -> (f32, f32, f32, f32) {
    let drawn = &list.drawn;
    let bounds = drawn.bounds;
    let row = drawn.metrics.row;
    let y = f32::from(bounds.origin.y) + drawn.grow.top(index, row) - drawn.scroll_y;
    let width = (f32::from(bounds.size.width) - drawn.metrics.lane).max(0.);
    (
        f32::from(bounds.origin.x),
        y,
        width,
        drawn.grow.height(index, row),
    )
}

/// Ipc.qml rowRect for any pane's list: the row's box while that list draws it, else empty.
pub(crate) fn drawn_row_rect(list: &ListState, held: &Held, index: usize) -> String {
    if list.drawn.listing != held.listing {
        return String::new();
    }
    let Some(shown) = list.visible().iter().find(|shown| shown.index == index) else {
        return String::new();
    };
    let (x, y, w, h) = drawn_row_box(list, shown.view);
    rect_of(x, y, w, h)
}

/// Ipc.qml listAreaRect for any pane's list.
pub(crate) fn list_area_rect(list: &ListState) -> String {
    rect_of_bounds(list.drawn.bounds)
}

pub(super) fn describe_row(facts: &flea::protocol::RowFacts, words: &format::Words) -> String {
    let kind = if facts.dir { "dir" } else { "file" };
    format!(
        "{}|{kind}|{}",
        facts.name,
        words.locale.format_size(facts.size)
    )
}

/// Header.cell(): where a title sits in header coordinates; a column that is not drawn collapses onto its right neighbour at width zero.
pub fn header_cell(
    layout: &Layout,
    metrics: &Metrics,
    row: f32,
    name: &str,
    dual: bool,
) -> Option<Cell> {
    if name == "name" {
        return HeaderState::title_cell(layout, metrics, None, dual);
    }
    let wanted = Column::from_key(name)?;
    let mut right = row - metrics.pad;
    for column in Column::DROP_ORDER {
        let cell = layout.cell(column);
        if column == wanted {
            return Some(cell.unwrap_or(Cell {
                x: right,
                width: 0.,
            }));
        }
        if let Some(cell) = cell {
            right = cell.x - metrics.gap;
        }
    }
    None
}

impl RowsAdapter<'_> {
    fn shown(&self, index: usize) -> Option<&Shown> {
        if self.list.drawn.listing != self.held.listing {
            return None;
        }
        self.list
            .visible()
            .iter()
            .find(|shown| shown.index == index)
    }

    fn palette(&self) -> Option<Palette> {
        self.list.drawn.palette
    }

    // A row the pane holds, and the states its owners give it.
    fn state(&self, index: usize) -> RowState {
        self.shown(index)
            .map(|shown| shown.state)
            .unwrap_or_default()
    }

    // Row.cellInk: the metadata ink, the foreground on a lifted row; a cut row ghosts only its mark, never its cells.
    fn cell_ink(&self, index: usize, palette: &Palette) -> Hsla {
        if self.state(index).lifted() {
            palette.foreground
        } else {
            palette.metadata
        }
    }

    // The row's box in window pixels: the list's left, its slot under the scroll, the row width less the lane.
    fn row_box(&self, index: usize) -> Option<(f32, f32, f32, f32)> {
        let shown = self.shown(index)?;
        Some(drawn_row_box(self.list, shown.view))
    }

    // Ipc.rowNameItem(): the name label of a drawn row, absent while the row is its own rename editor.
    fn name_box(&self, index: usize) -> Option<(f32, f32, f32, f32, f32)> {
        let shown = self.shown(index)?;
        if shown.state.renaming {
            return None;
        }
        let prepared = shown.prepared.as_ref()?;
        let (x, y, _, row) = self.row_box(index)?;
        let layout = &self.list.drawn.layout;
        let line = prepared.name.line();
        let height = f32::from(line.ascent + line.descent);
        let content = f32::from(line.width);
        let depth = self.held.row(index).map_or(0., |row| {
            crate::list::outline::indent(row, &self.list.drawn.metrics)
        });
        Some((
            x + layout.name.x + depth,
            y + (row - height) / 2.,
            (layout.name.width - depth).max(0.),
            height,
            content,
        ))
    }

    fn header_box(&self) -> Bounds<Pixels> {
        self.list.header.drawn.bounds
    }

    fn header_cell(&self, name: &str) -> Option<Cell> {
        let drawn = &self.list.drawn;
        let row = (f32::from(self.header_box().size.width) - drawn.metrics.lane).max(0.);
        header_cell(
            &self.list.header.drawn.layout,
            &drawn.metrics,
            row,
            name,
            self.list.view.dual,
        )
    }

    fn row_value(&self, function: &str, index: usize) -> Option<String> {
        let facts = self.held.row(index);
        let view = &self.list.view;
        if let Some(value) = super::outline::row(function, self.shown(index).and(facts)) {
            return Some(value);
        }
        let value = match function {
            "rowAt" => self
                .shown(index)
                .and_then(|shown| shown.prepared.as_ref())
                .and_then(|_| self.list.drawn_rows.row(index))
                .map_or_else(
                    || "loading".into(),
                    |facts| describe_row(facts, &view.words),
                ),
            // ui/Ipc.qml visibleRowMode: the drawn row's raw st_mode, -1 for a row with no built item.
            "visibleRowMode" => self
                .shown(index)
                .filter(|shown| shown.prepared.is_some())
                .and_then(|_| self.list.rows.facts(index))
                .map_or(-1, |facts| i64::from(facts.mode))
                .to_string(),
            "rowNamePx" => self
                .shown(index)
                .and_then(|shown| shown.prepared.as_ref())
                .map_or(-1, |prepared| {
                    round(f32::from(prepared.name.line().font_size))
                })
                .to_string(),
            "rowNameText" => self
                .shown(index)
                .filter(|shown| shown.prepared.is_some())
                .and_then(|_| self.list.rows.facts(index))
                .map(|facts| row::decorated_name(&crate::list::outline::display(facts)))
                .unwrap_or_default(),
            "rowSizeText" => self
                .shown(index)
                .filter(|_| self.list.drawn.layout.shown(Column::Size))
                .and_then(|shown| shown.prepared.as_ref())
                .map(|prepared| prepared.size_text.clone())
                .unwrap_or_default(),
            "rowClipMark" => match facts {
                None => "unavailable".into(),
                Some(facts) => match view.clip.mark_for(&facts.name) {
                    None => String::new(),
                    Some(ClipMark::Copy) => "copy".into(),
                    Some(ClipMark::Cut) => "scissors".into(),
                },
            },
            "rowGlyph" => self
                .shown(index)
                .map(|shown| {
                    if let Some(mark) = facts.and_then(|row| crate::list::outline::mark(view, row))
                    {
                        mark.name().to_owned()
                    } else if shown.prepared.is_none() {
                        Mark::File.name().to_owned()
                    } else {
                        self.list
                            .rows
                            .facts(index)
                            .map(|facts| Mark::for_row(&facts.icon, facts.mode).name().to_owned())
                            .unwrap_or_default()
                    }
                })
                .unwrap_or_default(),
            "rowNameColor" | "rowCellColor" | "rowDateColor" => {
                let (Some(prepared), Some(palette)) = (
                    self.shown(index).and_then(|shown| shown.prepared.as_ref()),
                    self.palette(),
                ) else {
                    return Some(String::new());
                };
                let lifted = self.state(index).lifted();
                let ink = match function {
                    "rowNameColor" => palette.name(prepared.ink, lifted),
                    "rowDateColor"
                        if view.today.is_some_and(|today| {
                            facts.is_some_and(|facts| format::is_recent(facts.mtime, today))
                        }) =>
                    {
                        palette.foreground
                    }
                    _ => self.cell_ink(index, &palette),
                };
                colour(ink)
            }
            "rowCellOverflow" => {
                let Some(prepared) = self.shown(index).and_then(|shown| shown.prepared.as_ref())
                else {
                    return Some(String::new());
                };
                let layout = &self.list.drawn.layout;
                let lines = [
                    &prepared.mode,
                    &prepared.size,
                    &prepared.date,
                    &prepared.kind,
                ];
                Column::ALL
                    .map(|column| {
                        let width = layout.cell(column).map_or(0., |cell| cell.width);
                        if f32::from(lines[column.slot()].line().width) > width {
                            "1"
                        } else {
                            "0"
                        }
                    })
                    .join("|")
            }
            "columnSet" => {
                let header = self.list.header.drawn.layout.names();
                let row = self
                    .shown(index)
                    .map(|_| self.list.drawn.layout.names())
                    .unwrap_or_default();
                format!("{header}|{row}")
            }
            "rowHovered" => self
                .shown(index)
                .is_some_and(|shown| shown.state.hovered)
                .to_string(),
            "rowRect" => self
                .row_box(index)
                .map(|(x, y, w, h)| rect_of(x, y, w, h))
                .unwrap_or_default(),
            // A part-drawn edge row's centre lies outside the list area; it names no row a click can reach.
            "rowCentre" => self
                .row_box(index)
                .filter(|_| facts.is_some())
                .filter(|&row| centre_inside(self.list.drawn.bounds, row))
                .map(|(x, y, w, h)| centre_of(x, y, w, h))
                .unwrap_or_default(),
            "rowLeft" => self
                .row_box(index)
                .filter(|_| facts.is_some())
                .map(|(x, ..)| round(x).to_string())
                .unwrap_or_default(),
            "rowNameRect" => self
                .name_box(index)
                .map(|(x, y, w, h, _)| rect_of(x, y, w, h))
                .unwrap_or_default(),
            "rowNameCentre" => self
                .name_box(index)
                .map(|(x, y, w, h, content)| {
                    format!("{} {}", round(x + w.min(content) / 2.), round(y + h / 2.))
                })
                .unwrap_or_default(),
            _ => return None,
        };
        Some(value)
    }
}

impl Adapter for RowsAdapter<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        let list = self.list;
        let value = match function {
            "outlineState" => super::outline::state(list),
            "headerTitles" => list.header.painted_titles.clone(),
            "headerCellRect" => self
                .header_cell(arguments.first()?)
                .map(|cell| format!("{}|{}", round(cell.x), round(cell.width)))
                .unwrap_or_default(),
            "headerCellCentre" => {
                let name = arguments.first()?;
                let bounds = self.header_box();
                let metrics = &list.drawn.metrics;
                // Header.qml's title item: the painted title's own span, in the header less the board's top padding.
                let column = (name != "name").then(|| Column::from_key(name)).flatten();
                self.header_cell(name)
                    .map(|cell| {
                        let cell = list.header.title_span(column, cell);
                        let top = f32::from(bounds.origin.y) + metrics.header_top;
                        centre_of(
                            f32::from(bounds.origin.x) + cell.x,
                            top,
                            cell.width,
                            metrics.header - metrics.header_top,
                        )
                    })
                    .unwrap_or_default()
            }
            "headerDragTrace" => list.header.drag.trace(),
            "headerLeft" => round(f32::from(self.header_box().origin.x)).to_string(),
            "headerTop" => round(f32::from(self.header_box().origin.y)).to_string(),
            "listAreaRect" => rect_of_bounds(list.drawn.bounds),
            "listingBackgroundCentre" => centre_of_bounds(list.drawn.bounds),
            "listContentY" => round(list.drawn.scroll_y).to_string(),
            _ => {
                // A negative or unreadable index names no row, which each reader answers the way QML's null item does.
                let index = arguments
                    .first()?
                    .parse::<i64>()
                    .ok()
                    .and_then(|at| usize::try_from(at).ok())
                    .unwrap_or(usize::MAX);
                let index = super::view::listing_row(function, index, list.drawn_filter.as_deref());
                return self.row_value(function, index).map(Reply::value);
            }
        };
        Some(Reply::value(value))
    }
}
