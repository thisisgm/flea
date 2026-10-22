//! The shipped Theme.font ladder, resolved at the window's current base size.
use super::Typeface;
use flea::ui_logic_st1::textsize;
use gpui::{font, px, App, Global, Window};

impl Global for Typeface {}

impl Typeface {
    /// Uses the resolved OEM family when available and the window's current base size.
    pub fn for_window(window: &Window, cx: &App) -> Self {
        let mut face = cx
            .try_global::<Self>()
            .cloned()
            .unwrap_or_else(Self::omarchy_default);
        let base = f32::from(window.rem_size()).round().max(1.) as u32;
        face.body = px(textsize::body(base) as f32);
        face.body_small = px(textsize::body_small(base) as f32);
        face.caption = px(textsize::caption(base) as f32);
        face
    }
}

impl Typeface {
    pub fn omarchy_default() -> Self {
        Self {
            font: font("JetBrainsMono Nerd Font"),
            body: px(14.),
            body_small: px(13.),
            caption: px(12.),
        }
    }
}
