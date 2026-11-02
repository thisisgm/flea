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
    let material = Material::from_colors(colors, true);
    material.apply(&mut lifted, alpha);
    let washes = [
        colors.foreground.alpha(0.),
        colors.accent.alpha(crate::list::CURSOR_WASH as f64),
        colors.foreground.alpha(crate::list::MARK_FILL as f64),
    ];
    // Text is tested against the actual glass plus the existing cursor/selection washes.
    for ink in [
        &mut lifted.foreground,
        &mut lifted.foregroundBright,
        &mut lifted.muted,
        &mut lifted.metadata,
        &mut lifted.metadataFrame,
        &mut lifted.mutedFrame,
        &mut lifted.symlink,
        &mut lifted.executable,
        &mut lifted.error,
        &mut lifted.errorOnSurface,
    ] {
        *ink = material.lift_over_washes(*ink, alpha, 4.5, &washes);
    }
    // Accent fills keep the original palette; only glyph roles are strengthened here.
    lifted.accent = colors.accent;
    lifted.accentFrame = colors.accentFrame;
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

pub(super) fn lookup(theme: &Theme, inks: [Hsla; 12], color: Hsla) -> Hsla {
    [
        (theme.foreground, inks[0]),
        (theme.foreground_bright, inks[1]),
        (theme.muted, inks[2]),
        (theme.metadata, inks[3]),
        (theme.metadata_frame, inks[4]),
        (theme.muted_frame, inks[5]),
        (theme.accent, inks[6]),
        (theme.accent_frame, inks[7]),
        (theme.symlink, inks[8]),
        (theme.executable, inks[9]),
        (theme.error, inks[10]),
        (theme.error_on_surface, inks[11]),
    ]
    .into_iter()
    .find_map(|(original, lifted)| (original == color).then_some(lifted))
    .unwrap_or(color)
}
