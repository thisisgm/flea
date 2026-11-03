//! The ui.json facts the Grid's pictures and marks read: thumbnail mode, class switches and language marks.
use super::is_image;
use flea::{jsondoc::Json, protocol::RowFacts};

/// ViewState.thumbnailMode: preview.thumbnails, absent reads media.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Off,
    Images,
    Media,
}

/// The ui.json facts Grid pictures and marks follow: the thumbnail mode, ExtThumbs.js's class switches and ViewState.langMarks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    pub mode: Mode,
    pub usb: bool,
    pub network: bool,
    pub phone: bool,
    pub languages: bool,
}

impl Policy {
    /// Sample input: {"preview":{"thumbnails":"images","thumbUsb":false},"langMarks":true}; absent keys read as v0.3.12 reads them.
    pub fn read(data: &Json) -> Self {
        let preview = data.get("preview");
        let flag = |key| preview.and_then(|p| p.get(key)).and_then(Json::as_bool);
        Self {
            mode: match preview
                .and_then(|p| p.get("thumbnails"))
                .and_then(Json::as_str)
            {
                Some("off") => Mode::Off,
                Some("images") => Mode::Images,
                _ => Mode::Media,
            },
            usb: flag("thumbUsb") != Some(false),
            network: flag("thumbNetwork") == Some(true),
            phone: flag("thumbPhone") == Some(true),
            languages: data.get("langMarks").and_then(Json::as_bool) != Some(false),
        }
    }

    /// ExtThumbs.cacheOnly: a class whose switch is off asks the shared cache only, so pictures already made still draw.
    pub fn cache_only(&self, class: &str) -> bool {
        let off = match class {
            "usb" => !self.usb,
            "network" => !self.network,
            "phone" => !self.phone,
            _ => false,
        };
        off && self.mode != Mode::Off
    }

    /// Thumbs.allowed: a row the backend flagged "t":false has no thumbnailer, so it is never asked.
    pub fn allowed(&self, row: &RowFacts) -> bool {
        !row.dir
            && !row.thumbless
            && match self.mode {
                Mode::Off => false,
                Mode::Images => is_image(row),
                Mode::Media => true,
            }
    }
}
