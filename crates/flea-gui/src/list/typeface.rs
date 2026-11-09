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
    /// A Shell window's accepted snapshot; any other window takes the installed settings face, or the rem size only when none is installed.
    pub fn for_window(window: &Window, cx: &App) -> Self {
        if let Some(root) = window.root::<crate::window::root::Root>().flatten() {
            return root.read(cx).shell.read(cx).active_typeface();
        }
        if let Some(shell) = window.root::<crate::state::Shell>().flatten() {
            return shell.read(cx).active_typeface();
        }
        if let Some(face) = cx.try_global::<Self>() {
            return face.clone();
        }
        Self::omarchy_default().at_base(f32::from(window.rem_size()).round().max(1.) as u32)
    }

    /// This face at a text size, its tokens derived as an explicit text size derives them.
    pub fn at_base(mut self, base: u32) -> Self {
        self.base = px(base as f32);
        self.body = px(flea::ui_logic_settings_pages::textsize::body(base) as f32);
        self.body_small = px(flea::ui_logic_settings_pages::textsize::body_small(base) as f32);
        self.caption = px(flea::ui_logic_settings_pages::textsize::caption(base) as f32);
        self
    }
}
