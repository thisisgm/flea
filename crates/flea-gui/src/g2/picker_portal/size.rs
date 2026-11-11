//! The chooser at Flea's text size: the face's type tokens, and the board lengths scaled as v0.3.12's Theme.space scales them.
use crate::list::Typeface;
use gpui::{px, App, Pixels, Window};

#[derive(Clone, Copy)]
pub(super) struct Size {
    pub body: f32,
    pub body_small: f32,
    pub caption: f32,
    // Theme.sizeRatio: the effective base over Omarchy's own, 1 while Flea follows Omarchy.
    ratio: f32,
}

impl Size {
    pub(super) fn of(window: &Window, cx: &App) -> Self {
        let oem = cx
            .try_global::<crate::presentation::Fonts>()
            .map_or(0, |fonts| fonts.base);
        Self::at(&Typeface::for_window(window, cx), oem)
    }

    fn at(face: &Typeface, oem: u32) -> Self {
        let base = f32::from(face.base);
        Self {
            body: f32::from(face.body),
            body_small: f32::from(face.body_small),
            caption: f32::from(face.caption),
            ratio: if oem > 0 && base > 0. {
                base / oem as f32
            } else {
                1.
            },
        }
    }

    /// A board length at this text size, rounded as Theme.space rounds it.
    pub(super) fn len(self, board: f32) -> f32 {
        (board * self.ratio).round()
    }

    pub(super) fn px(self, board: f32) -> Pixels {
        px(self.len(board))
    }
}
