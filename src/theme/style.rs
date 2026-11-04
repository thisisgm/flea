use super::parse::{boolean, float_prefix, integer};
use super::Values;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct Font {
    pub family: String,
    pub resolved_family: String,
    pub base_size: u32,
    pub sizes: BTreeMap<&'static str, u32>,
}

impl Font {
    pub fn resolve(values: &Values, stop: Option<u32>, resolved_family: &str) -> Self {
        let base = integer(values, "font.base-size").unwrap_or(12.0).max(1.0) as u32;
        let size = stop.unwrap_or(base).max(1);
        let mut sizes = BTreeMap::new();
        for (name, key, mult) in [
            ("caption", "caption", 0.833),
            ("bodySmall", "body-small", 0.917),
            ("body", "body", 1.0),
            ("subtitle", "subtitle", 1.083),
            ("title", "title", 1.167),
            ("heading", "heading", 1.333),
            ("display", "display", 2.0),
            ("displayLarge", "display-large", 2.333),
            ("iconLarge", "icon-large", 1.5),
        ] {
            let pinned = if stop.is_none() {
                integer(values, &format!("font.{key}"))
            } else {
                None
            };
            let px = pinned
                .filter(|v| *v > 0.0)
                .unwrap_or(size as f64 * mult)
                .round() as u32;
            sizes.insert(name, px.max(1));
        }
        for (name, key, fallback) in [
            ("iconSmall", "icon-small", "bodySmall"),
            ("icon", "icon", "title"),
        ] {
            let pinned = if stop.is_none() {
                integer(values, &format!("font.{key}"))
            } else {
                None
            };
            sizes.insert(
                name,
                pinned
                    .filter(|v| *v > 0.0)
                    .unwrap_or(sizes[fallback] as f64)
                    .round() as u32,
            );
        }
        Self {
            family: "monospace".into(),
            resolved_family: resolved_family.into(),
            base_size: size,
            sizes,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Spacing {
    pub scale: f64,
    pub pixels: BTreeMap<&'static str, u32>,
}

impl Spacing {
    pub fn resolve(values: &Values, base_size: u32) -> Self {
        let scale = float_prefix(values, "spacing.scale", 1.0);
        let scale = if scale < 0.0 { 1.0 } else { scale };
        let scale = scale
            * if boolean(values, "spacing.scale-with-font", true) {
                base_size as f64 / 12.0
            } else {
                1.0
            };
        let mut out = Self {
            scale,
            pixels: BTreeMap::new(),
        };
        out.pixels.insert("hairline", out.space(1.0));
        for (name, key, fallback) in [
            ("xxs", "xxs", 2.0),
            ("xs", "xs", 3.0),
            ("sm", "sm", 4.0),
            ("md", "md", 6.0),
            ("lg", "lg", 8.0),
            ("xl", "xl", 10.0),
            ("xxl", "xxl", 12.0),
            ("xxxl", "xxxl", 14.0),
            ("huge", "huge", 18.0),
            ("controlGap", "control-gap", 8.0),
            ("controlPaddingX", "control-padding-x", 10.0),
            ("controlPaddingY", "control-padding-y", 6.0),
            ("inputPaddingY", "input-padding-y", 7.0),
            ("controlHeight", "control-height", 28.0),
            ("popupRowHeight", "popup-row-height", 28.0),
            ("dropdownWidth", "dropdown-width", 240.0),
            (
                "searchableDropdownWidth",
                "searchable-dropdown-width",
                260.0,
            ),
            ("numberFieldWidth", "number-field-width", 120.0),
            (
                "searchablePopupMinHeight",
                "searchable-popup-min-height",
                220.0,
            ),
            ("rowGap", "row-gap", 8.0),
            ("rowPaddingX", "row-padding-x", 12.0),
            ("labelGap", "label-gap", 4.0),
            ("panelGap", "panel-gap", 14.0),
            ("panelPadding", "panel-padding", 18.0),
            ("popupPadding", "popup-padding", 14.0),
        ] {
            let n = float_prefix(values, &format!("spacing.{key}"), -1.0);
            let px = if n >= 0.0 {
                n.round() as u32
            } else {
                out.space(fallback)
            };
            out.pixels.insert(name, px);
        }
        out
    }

    pub fn space(&self, px: f64) -> u32 {
        if px <= 0.0 || self.scale <= 0.0 {
            0
        } else {
            (px * self.scale).round().max(1.0) as u32
        }
    }
}

// Hyprland options are supplied by the caller after configreloaded, never read on a paint path.
#[derive(Clone, Copy, Debug, Default)]
pub struct Decoration {
    pub corner_radius: u32,
    pub gaps_out: u32,
    pub reduced_motion: bool,
}

impl Decoration {
    pub fn apply_rounding(&mut self, value: f64) {
        if value.is_finite() && value >= 0.0 {
            self.corner_radius = value.round() as u32;
        }
    }

    pub fn apply_gaps(&mut self, value: f64) {
        if value.is_finite() && value >= 0.0 {
            self.gaps_out = (value / 2.0).round() as u32;
        }
    }
}
