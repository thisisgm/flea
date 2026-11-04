//! HIG frame roles shared by cards, readers, menus and their controls.
use crate::list::Palette;
use gpui::Hsla;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frames {
    pub ink: Hsla,
    pub radius: f32,
}

impl Frames {
    pub fn from_palette(palette: &Palette, radius: f32) -> Self {
        Self {
            ink: palette.muted_frame,
            radius: crate::compositor::valid_corner(f64::from(radius)),
        }
    }
}
