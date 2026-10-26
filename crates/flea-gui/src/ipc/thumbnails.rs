//! Thumbnail readers describe the active painted row's source, raw file and decoder state.
use super::Adapter;
use crate::{
    columns::{geometry::Slot, TrackRows},
    window::{pane::PaneHost, startup::View},
};
use flea::ipc::wire::Reply;

pub(crate) struct Thumbnails<'a> {
    pub host: &'a PaneHost,
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

impl Adapter for Thumbnails<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
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
