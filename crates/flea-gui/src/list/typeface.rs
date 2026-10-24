use super::ListState;
use gpui::{font, px, App, Font, Global, Pixels, Window};

/// The resolved face and OEM font tokens shared by list geometry and every mounted surface.
#[derive(Clone, Debug, PartialEq)]
pub struct Typeface {
    pub font: Font,
    pub base: Pixels,
    pub body: Pixels,
    pub body_small: Pixels,
    pub caption: Pixels,
}

impl Typeface {
    pub fn omarchy_default() -> Self {
        Self {
            font: font("JetBrainsMono Nerd Font"),
            base: px(14.),
            body: px(14.),
            body_small: px(13.),
            caption: px(12.),
        }
    }
}

impl ListState {
    pub fn set_typeface(&mut self, typeface: Typeface) -> bool {
        if self.typeface == typeface {
            return false;
        }
        self.typeface = typeface;
        self.invalidate_text();
        self.header.forget_titles();
        true
    }
}

impl Global for Typeface {}

impl Typeface {
    /// Mounted surfaces use their own window's accepted font snapshot, including explicit text size.
    pub fn for_window(window: &Window, cx: &App) -> Self {
        if let Some(root) = window.root::<crate::window::root::Root>().flatten() {
            return root.read(cx).shell.read(cx).active_typeface();
        }
        if let Some(shell) = window.root::<crate::state::Shell>().flatten() {
            return shell.read(cx).active_typeface();
        }
        let mut face = cx
            .try_global::<Self>()
            .cloned()
            .unwrap_or_else(Self::omarchy_default);
        let base = f32::from(window.rem_size()).round().max(1.) as u32;
        face.base = px(base as f32);
        face.body = px(flea::ui_logic_st1::textsize::body(base) as f32);
        face.body_small = px(flea::ui_logic_st1::textsize::body_small(base) as f32);
        face.caption = px(flea::ui_logic_st1::textsize::caption(base) as f32);
        face
    }
}
