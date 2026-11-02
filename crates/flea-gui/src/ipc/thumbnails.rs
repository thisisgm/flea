//! Thumbnail readers describe the active painted row's source, raw file and decoder state.
use super::{rows::rect_of, Adapter};
use crate::{
    columns::{geometry::Slot, TrackRows},
    grid::element::pictures::policy::{Mode, Policy},
    window::{pane::PaneHost, startup::View},
};
use flea::ipc::wire::Reply;

pub(crate) struct Thumbnails<'a> {
    pub host: &'a PaneHost,
    /// The preview.thumbnails policy the pictures follow, read from the settings document.
    pub policy: Policy,
    pub preview: crate::window::selection_preview::thumbnail_observer::Snapshot,
}

// ViewState.thumbnailMode: the preview.thumbnails setting by its own name.
fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "off",
        Mode::Images => "images",
        Mode::Media => "media",
    }
}

// Pane.viewMode: the Dual layout is two list panes, so it reports list.
fn view_name(view: Option<View>) -> &'static str {
    match view {
        Some(View::List | View::Dual) => "list",
        Some(View::Columns) => "columns",
        Some(View::Grid) => "grid",
        None => "",
    }
}

impl Thumbnails<'_> {
    fn visible(&self, index: usize) -> bool {
        let held = &self.host.pane.held;
        match self.host.drawn_view {
            Some(View::List | View::Dual) => {
                let list = self.host.list.borrow();
                list.drawn.listing == held.listing
                    && list
                        .visible()
                        .iter()
                        .any(|row| row.index == index && row.prepared.is_some())
            }
            Some(View::Grid) => self.host.grid.as_ref().is_some_and(|grid| {
                grid.state
                    .borrow()
                    .drawn_row(index)
                    .and_then(|row| row.name())
                    .is_some_and(|name| held.row(index).is_some_and(|facts| facts.name == name))
            }),
            Some(View::Columns) => self
                .host
                .columns
                .as_ref()
                .and_then(|columns| columns.painters.get(Slot::Active))
                .is_some_and(|paint| {
                    let paint = paint.borrow();
                    paint.drawn.as_ref().is_some_and(|drawn| {
                        matches!(drawn.view().rows, TrackRows::Listing)
                            && drawn
                                .held
                                .as_ref()
                                .is_some_and(|drawn| drawn.listing == held.listing)
                    }) && paint.visible().any(|row| row == index)
                }),
            None => false,
        }
    }
}

impl Thumbnails<'_> {
    // The mark slot a drawn row's thumbnail fills, as window pixels (x, y, width, height); a Dual pane fits the mark's own side.
    fn thumb_box(&self, index: usize) -> Option<(f32, f32, f32, f32)> {
        match self.host.drawn_view {
            Some(View::List | View::Dual) => {
                let list = self.host.list.borrow();
                let drawn = &list.drawn;
                let shown = list.visible().iter().find(|shown| shown.index == index)?;
                let metrics = &drawn.metrics;
                let side = if self.host.drawn_view == Some(View::Dual) {
                    metrics.mark
                } else {
                    metrics.slot
                };
                let left = f32::from(drawn.bounds.origin.x);
                let top = f32::from(drawn.bounds.origin.y) + shown.view as f32 * metrics.row
                    - drawn.scroll_y;
                Some((
                    left + metrics.pad + (metrics.slot - side) / 2.,
                    top + (metrics.row - side) / 2.,
                    side,
                    side,
                ))
            }
            Some(View::Columns) => {
                let columns = self.host.columns.as_ref()?;
                let paint = columns.painters.get(Slot::Active)?.borrow();
                let (x, y, side) = paint.slot_box(index)?;
                Some((x, y, side, side))
            }
            _ => None,
        }
    }
}

impl Thumbnails<'_> {
    // ui/Ipc.qml rowClipMark: "unavailable" for a row the frame did not draw, else "", "copy" or "scissors".
    fn clip_mark(&self, index: usize) -> &'static str {
        let name = match self.host.drawn_view {
            Some(View::Grid) => self.host.grid.as_ref().and_then(|grid| {
                let state = grid.state.borrow();
                state.drawn_row(index).and_then(|row| row.name())?;
                let held = state.frame_held.as_deref()?;
                held.row(index).map(|row| row.name.clone())
            }),
            Some(View::Columns) => self.host.columns.as_ref().and_then(|columns| {
                let paint = columns.painters.get(Slot::Active)?.borrow();
                paint.prepared(index)?;
                let drawn = paint.drawn.as_ref()?;
                if !matches!(drawn.view().rows, TrackRows::Listing)
                    || drawn.view().path != self.host.path
                {
                    return None;
                }
                drawn
                    .held
                    .as_deref()?
                    .row(index)
                    .map(|row| row.name.clone())
            }),
            _ => None,
        };
        let Some(name) = name else {
            return "unavailable";
        };
        match self.host.list.borrow().view.clip.mark_for(&name) {
            None => "",
            Some(crate::row::marks::ClipMark::Copy) => "copy",
            Some(crate::row::marks::ClipMark::Cut) => "scissors",
        }
    }
}

impl Adapter for Thumbnails<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function == "thumbnailPolicyState" {
            return Some(Reply::value(self.policy_state()));
        }
        // The grid answers rowThumbRect from its own slots, so the pictures share it only in the list and columns.
        if function == "rowThumbRect" && self.host.drawn_view != Some(View::Grid) {
            let index = arguments
                .first()
                .and_then(|text| text.parse::<usize>().ok());
            let rect = index
                .filter(|&row| self.visible(row))
                .and_then(|row| self.thumb_box(row))
                .map_or_else(String::new, |(x, y, width, height)| {
                    rect_of(x, y, width, height)
                });
            return Some(Reply::value(rect));
        }
        // The grid answers rowThumbReady from its own decoded tiles, so the pictures share it only in the list and columns.
        if function == "rowThumbReady" && self.host.drawn_view != Some(View::Grid) {
            let index = arguments
                .first()
                .and_then(|text| text.parse::<usize>().ok());
            let held = &self.host.pane.held;
            let visible = index.is_some_and(|row| self.visible(row));
            let ready = index.is_some_and(|row| self.host.list.borrow().pictures.ready(held, row));
            return Some(Reply::value((visible && ready).to_string()));
        }
        // The List answers rowClipMark from its drawn rows; Grid and Columns read the drawn row through the pane's clipboard marks.
        if function == "rowClipMark"
            && matches!(self.host.drawn_view, Some(View::Grid | View::Columns))
        {
            let index = arguments
                .first()
                .and_then(|text| text.parse::<usize>().ok());
            return Some(Reply::value(
                index.map_or("unavailable", |row| self.clip_mark(row)),
            ));
        }
        if !matches!(
            function,
            "thumbFile" | "rowIcon" | "rowIconStatus" | "rowThumb"
        ) {
            return None;
        }
        let index = arguments
            .first()
            .and_then(|text| text.parse::<usize>().ok());
        let held = &self.host.pane.held;
        let list = self.host.list.borrow();
        let file = index.map_or_else(String::new, |row| list.pictures.file(held, row));
        let visible = index.is_some_and(|row| self.visible(row));
        let value = match function {
            "thumbFile" => file,
            "rowThumb" => {
                if visible {
                    file
                } else {
                    String::new()
                }
            }
            "rowIcon" => index
                .and_then(|row| held.row(row))
                .filter(|_| visible && !file.is_empty())
                .map_or_else(String::new, |facts| {
                    format!(
                        "{}?m={}",
                        flea::ui_logic_mn1::copyas::file_uri(&file),
                        facts.mtime
                    )
                }),
            "rowIconStatus" => index
                .filter(|_| visible)
                .map_or(-1, |row| list.pictures.status(held, row))
                .to_string(),
            _ => String::new(),
        };
        Some(Reply::value(value))
    }
}

#[path = "thumbnails/policy.rs"]
mod policy;
