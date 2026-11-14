//! The theme roles the Trash surface draws with, one plain-data seam until G1 publishes the live theme snapshot.
use crate::list::Palette;
use gpui::{Hsla, Pixels};

/// Omarchy roles by their board names: --bg, --surface, --fg, --muted, --muted-frame, --accent, --accent-frame,
/// --mark-fill and --error, plus the compositor's corner radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    pub background: Hsla,
    pub surface: Hsla,
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
    /// The list's metadata ink, which its column header titles wear.
    pub metadata: Hsla,
    pub accent: Hsla,
    pub accent_frame: Hsla,
    pub mark_fill: Hsla,
    pub error: Hsla,
    pub radius: Pixels,
}

impl Ink {
    /// Stand-in from the list pane's interim palette; the G1 theme snapshot replaces every derived role.
    pub fn interim(palette: &Palette, radius: Pixels) -> Self {
        Self {
            background: palette.background,
            surface: palette.background,
            foreground: palette.foreground,
            muted: palette.muted,
            muted_frame: palette.muted_frame,
            metadata: palette.metadata,
            accent: palette.accent,
            accent_frame: palette.accent_frame,
            mark_fill: palette.marked,
            error: palette.error,
            radius,
        }
    }

    /// Board `.fb --hair`: foreground at 12%.
    pub fn hair(&self) -> Hsla {
        self.foreground.opacity(super::board::HAIR)
    }
    /// Controls under 24 take half the compositor radius.
    pub fn small_radius(&self) -> Pixels {
        self.radius / 2.
    }
    /// The cursor's edge, drawn in the accent frame.
    pub fn edge(&self) -> Hsla {
        self.accent_frame
    }
}
