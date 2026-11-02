//! Cached contrast recipes for the independent chrome and listing producers.
use crate::palette::Theme;
use flea::theme::{Color, Material, ThemeColors};
use gpui::{Hsla, Rgba};

pub(super) fn role(color: Color) -> Hsla {
    Rgba {
        r: color.r as f32,
        g: color.g as f32,
        b: color.b as f32,
        a: color.a as f32,
    }
    .into()
}

pub(super) fn inks(colors: &ThemeColors, alpha: u8) -> [Hsla; 12] {
    let mut lifted = colors.clone();
    Material::from_colors(colors, true).apply(&mut lifted, alpha);
    [
        lifted.foreground,
        lifted.foregroundBright,
        lifted.muted,
        lifted.metadata,
        lifted.metadataFrame,
        lifted.mutedFrame,
        lifted.accent,
        lifted.accentFrame,
        lifted.symlink,
        lifted.executable,
        lifted.error,
        lifted.errorOnSurface,
    ]
    .map(role)
}

pub(super) fn apply(theme: &mut Theme, colors: [Hsla; 12]) {
    [
        theme.foreground,
        theme.foreground_bright,
        theme.muted,
        theme.metadata,
        theme.metadata_frame,
        theme.muted_frame,
        theme.accent,
        theme.accent_frame,
        theme.symlink,
        theme.executable,
        theme.error,
        theme.error_on_surface,
    ] = colors;
}
