//! design-hig boards trash, trash-strip-focus, trash-strip-hover, trash-menu and trash-emptying, as numbers.
use gpui::SharedString;

/// The strip, `.chrome`: 27 tall, 14 padding, 9 gap, caption 12, a hairline under it.
pub const STRIP_H: f32 = 27.;
pub const PAD_X: f32 = 14.;
pub const GAP: f32 = 9.;
/// `.cbtn`: a 24 by 27 slot around a 16 glyph.
pub const CBTN_W: f32 = 24.;
pub const CBTN_GLYPH: f32 = 16.;
/// Empty Trash…: 20 tall, caption 12, 9 padding, a hairline frame, half the compositor radius.
pub const BUTTON_H: f32 = 20.;
pub const BUTTON_PAD: f32 = 9.;
/// The column header: 27 tall, caption 12 in foreground, a 19 mark slot, Original Location 210, Deleted 135.
pub const HEADER_H: f32 = 27.;
pub const SLOT: f32 = 19.;
pub const LOCATION_W: f32 = 210.;
pub const DELETED_W: f32 = 135.;
/// `.trashv .row`: 31 tall, a 19 glyph in a 19 slot, body 14 name and cells, each cell 9 after the last.
pub const ROW_H: f32 = 31.;
pub const GLYPH: f32 = 19.;
pub const EDGE: f32 = 2.;
/// The trash-menu board opens the item's menu 3 under its row.
pub const MENU_DROP: f32 = 3.;
pub const BODY: f32 = 14.;
pub const CAPTION: f32 = 12.;
/// Washes over foreground or accent: hairline 12%, pointer 8%, a press 14% (Buttons.js), the cursor 18%.
pub const HAIR: f32 = 0.12;
pub const HOVER: f32 = 0.08;
pub const PRESS: f32 = 0.14;
pub const WASH: f32 = 0.18;
/// An unavailable control keeps its place at 0.55 (rule 14).
pub const DISABLED: f32 = 0.55;

macro_rules! glyph {
    ($key:literal, $d:literal) => {
        (
            $key,
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" "#,
                r#"stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d=""#,
                $d,
                r#""/></svg>"#
            )
            .as_bytes(),
        )
    };
}

/// The strip's and the menus' marks, path data exactly as the boards draw them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Back,
    Up,
    FolderOpen,
    Undo,
    Trash,
}

impl Glyph {
    pub fn named(name: &str) -> Option<Self> {
        Some(match name {
            "arrow-left" => Self::Back,
            "arrow-up" => Self::Up,
            "folder-open" => Self::FolderOpen,
            "undo" => Self::Undo,
            "trash" => Self::Trash,
            _ => return None,
        })
    }

    pub fn svg(self) -> (SharedString, &'static [u8]) {
        let (key, data) = match self {
            Self::Back => glyph!("flea-pl3:arrow-left", "M19 12H5 M12 19l-7-7 7-7"),
            Self::Up => glyph!("flea-pl3:arrow-up", "M12 19V5 M5 12l7-7 7 7"),
            Self::FolderOpen => glyph!("flea-pl3:folder-open", "M2 20V3h6l2 3h12v3 M22 11l-2.5 9H2l2.5-9z"),
            Self::Undo => glyph!("flea-pl3:undo", "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5"),
            Self::Trash => glyph!("flea-pl3:trash", "M3 6h18 M8 6V3h8v3 M6 6l1.2 15h9.6L18 6 M10 10v7 M14 10v7"),
        };
        (SharedString::new_static(key), data)
    }
}
