//! Thumbnail readers describe the active painted row's source, raw file and decoder state.
use super::Adapter;
use crate::{
    columns::{geometry::Slot, TrackRows},
    grid::element::pictures::policy::Mode,
    window::{pane::PaneHost, startup::View},
};
use flea::ipc::wire::Reply;

pub(crate) struct Thumbnails<'a> {
    pub host: &'a PaneHost,
    /// The preview.thumbnails policy the pictures follow, read from the settings document.
    pub mode: Mode,
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

impl Adapter for Thumbnails<'_> {
    fn call(&self, function: &str, arguments: &[String]) -> Option<Reply> {
        if function == "thumbnailPolicyState" {
            return Some(Reply::value(self.policy_state()));
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

impl Thumbnails<'_> {
    // The v0.3.12 keys. GPUI keeps no per-file thumbState map or preview column owner, so files, pending and the preview read idle.
    fn policy_state(&self) -> String {
        let content_y = match self.host.drawn_view {
            Some(View::List | View::Dual) => self.host.list.borrow().drawn.scroll_y.round() as i64,
            Some(View::Grid) => self
                .host
                .grid
                .as_ref()
                .map_or(0, |grid| grid.state.borrow().scroll.round() as i64),
            _ => 0,
        };
        format!(
            "{{\"view\":\"{}\",\"mode\":\"{}\",\"files\":{{}},\"pending\":[],\"contentY\":{content_y},\"cursor\":{},\"previewIndex\":-1,\"previewPath\":\"\",\"previewReady\":false}}",
            view_name(self.host.drawn_view),
            mode_name(self.mode),
            self.host.pane.cursor,
        )
    }
}
