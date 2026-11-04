use super::{Theme, TAG_NAMES};
use std::collections::BTreeMap;

impl Theme {
    pub fn tokens(&self) -> BTreeMap<String, String> {
        let c = &self.colors;
        let mut out = BTreeMap::new();
        macro_rules! colors { ($($name:ident),*) => { $(out.insert(concat!("color.", stringify!($name)).into(), c.$name.argb());)* }; }
        colors!(
            background,
            foreground,
            opaqueForegroundBright,
            foregroundBright,
            opaqueMuted,
            muted,
            opaqueMutedFrame,
            mutedFrame,
            opaqueMetadata,
            metadata,
            opaqueMetadataFrame,
            metadataFrame,
            accent,
            opaqueError,
            error,
            opaqueErrorOnSurface,
            errorOnSurface,
            surface,
            opaqueSymlink,
            symlink,
            opaqueExecutable,
            executable,
            opaqueAccentFrame,
            accentFrame
        );
        out.insert("color.accentHasHue".into(), c.accentHasHue.to_string());
        out.insert("ready".into(), self.ready.to_string());
        out.insert("baseSize".into(), self.font.base_size.to_string());
        out.insert("font.family".into(), self.font.family.clone());
        for (name, px) in &self.font.sizes {
            out.insert(format!("font.{name}"), px.to_string());
        }
        for (name, px) in &self.spacing.pixels {
            out.insert(format!("spacing.{name}"), px.to_string());
        }
        out.insert("spacing.scale".into(), format!("{:.6}", self.spacing.scale));
        for (name, color) in &self.fills {
            out.insert(name.clone(), color.argb());
        }
        for (name, n) in &self.numbers {
            out.insert(name.clone(), format!("{n:.6}"));
        }
        out.insert(
            "cornerRadius".into(),
            self.decoration.corner_radius.to_string(),
        );
        out.insert("gapsOut".into(), self.decoration.gaps_out.to_string());
        out.insert(
            "reducedMotion".into(),
            self.decoration.reduced_motion.to_string(),
        );
        out.insert("tag.fallback".into(), self.tags.fallback.to_string());
        out.insert("tag.swatchSize".into(), self.tags.swatch_size.to_string());
        for (i, name) in TAG_NAMES.iter().enumerate() {
            out.insert(
                format!("tag.colors.{name}"),
                self.tags.colors[i].map(|c| c.argb()).unwrap_or_default(),
            );
            out.insert(
                format!("tag.menuColors.{name}"),
                self.tags.menu_colors[i]
                    .map(|c| c.argb())
                    .unwrap_or_default(),
            );
        }
        out
    }
}
