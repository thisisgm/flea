//! design-hig boards trash, trash-strip-focus, trash-strip-hover, trash-menu and trash-emptying, as numbers.
use crate::list::{Metrics, Typeface};
use gpui::{Pixels, SharedString};

/// The flash card scales this 27 px board strip height with its type.
pub const STRIP_H: f32 = 27.;
/// `.cbtn`: a slot at least 24 wide around a 16 glyph.
pub const CBTN_W: f32 = 24.;
pub const CBTN_GLYPH: f32 = 16.;
/// The column header's Original Location 210 at bodySmall 13 and Deleted 135.
pub const LOCATION_W: f32 = 210.;
pub const DELETED_W: f32 = 135.;
/// The trash-menu board opens the item's menu 3 under its row.
pub const MENU_DROP: f32 = 3.;
const BOARD_BODY: f32 = 14.;
const BOARD_BODY_SMALL: f32 = 13.;

/// The Trash view's measures at a text size, as TrashView.qml takes them from Theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Board {
    /// The strip, `.chrome`: 27 tall at 14, row padding, the gap, caption type, a hairline under it.
    pub strip_h: f32,
    pub pad_x: f32,
    pub gap: f32,
    pub cbtn_w: f32,
    pub cbtn_glyph: f32,
    /// Empty Trash…: Theme.chromeControlHeight, 20 tall at 14, with the gap as its padding.
    pub button_h: f32,
    pub button_pad: f32,
    /// The column header, the strip's height again, a mark slot, then Original Location and Deleted.
    pub header_h: f32,
    pub slot: f32,
    /// The list's own icon slot, wider than the mark it centres; the rows take it under a unified look.
    pub list_slot: f32,
    /// The list header's padding above its titles; a unified header sets its titles below it as the list does.
    pub header_top: f32,
    pub location_w: f32,
    pub deleted_w: f32,
    /// `.trashv .row`: the list's row and mark, a two-hairline accent edge, body type.
    pub row_h: f32,
    pub glyph: f32,
    pub edge: f32,
    pub menu_drop: f32,
    pub body: Pixels,
    pub caption: Pixels,
}

impl Board {
    pub fn at(face: &Typeface) -> Self {
        let metrics = Metrics::for_typeface(face);
        let body = f32::from(face.body);
        let board = |value: f32| (value * body / BOARD_BODY).round();
        let inset = (f32::from(face.caption) / 4.).round().max(2.);
        let cbtn_glyph = board(CBTN_GLYPH);
        let mark = metrics.mark.round();
        Self {
            strip_h: metrics.header,
            pad_x: metrics.pad,
            gap: metrics.gap,
            cbtn_w: CBTN_W.max(cbtn_glyph),
            cbtn_glyph,
            button_h: metrics.header - metrics.hairline - 2. * inset,
            button_pad: metrics.gap,
            header_h: metrics.header,
            slot: mark,
            list_slot: metrics.slot.round(),
            header_top: metrics.header_top,
            location_w: (LOCATION_W * f32::from(face.body_small) / BOARD_BODY_SMALL).round(),
            deleted_w: board(DELETED_W),
            row_h: metrics.row,
            glyph: mark,
            // TrashView.qml: the marked row's edge is two hairlines at every size.
            edge: 2. * metrics.hairline,
            menu_drop: board(MENU_DROP),
            body: face.body,
            caption: face.caption,
        }
    }

    /// The strip and the header above the rows.
    pub fn top(&self) -> f32 {
        self.strip_h + self.header_h
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::at(&Typeface::omarchy_default())
    }
}

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
            Self::FolderOpen => glyph!(
                "flea-pl3:folder-open",
                "M2 20V3h6l2 3h12v3 M22 11l-2.5 9H2l2.5-9z"
            ),
            Self::Undo => glyph!(
                "flea-pl3:undo",
                "M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5"
            ),
            Self::Trash => glyph!(
                "flea-pl3:trash",
                "M3 6h18 M8 6V3h8v3 M6 6l1.2 15h9.6L18 6 M10 10v7 M14 10v7"
            ),
        };
        (SharedString::new_static(key), data)
    }
}
