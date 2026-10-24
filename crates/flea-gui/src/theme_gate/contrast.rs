use crate::{g2::op1::palette::CardPalette, list::Palette, palette::Theme};
use gpui::{Hsla, Rgba, TextStyle};
use std::collections::BTreeMap;

// DESIGN-040 "Contrast exceptions", exact legacy allowances, never expanded by this gate.
// "*:tag * on a marked row=110": Tags callout 2 lifts against the unselected ground.
// "*:tag * on the cursor row=42": Tags callout 2 lifts against the unselected ground.
// "*:tag orange=3": last-horizon, solitude and white use digit squares for a missing orange.
// "*:foreground text on a marked row=13": rule 7 preserves OEM Style.selectionFill.
// "*:muted text on surface=4": the older 3:1 recipe predates Theme.qml's 4.5 text lift.
// "*:muted glyph on surface=4": catppuccin-latte, flexoki-light, lupine and rose-pine, 2.66 to 2.70.
// "*:rule-12 metadata ink on background=3": Qt.darker(foreground, 1.4) is retained for glyphs.
// The current QML sweep does not assert the first four pairs or rule-12 glyph text contrast.
// Its muted *text* now holds 4.5 on both planes; legacy allowances cannot waive that floor.

fn rgb(colour: Hsla) -> [f64; 3] {
    let c: Rgba = colour.into();
    [c.r as f64, c.g as f64, c.b as f64]
}

fn over(ink: Hsla, ground: [f64; 3]) -> [f64; 3] {
    let a = ink.a as f64;
    let c = rgb(ink);
    std::array::from_fn(|i| ((c[i] * a + ground[i] * (1. - a)) * 255.).round() / 255.)
}

// Independent WCAG sRGB oracle, including the alpha the real painter composites.
fn ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    let luminance = |c: [f64; 3]| {
        let linear = c.map(|v| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn contrast(ink: Hsla, ground: Hsla) -> f64 {
    ratio(over(ink, rgb(ground)), rgb(ground))
}

// Recorded opaque glass floors from tests/theme-contrast.qml::glassFloors, including both fixtures.
fn recorded_floor(name: &str) -> u16 {
    match name {
        "catppuccin" | "last-horizon" | "matte-black" | "sunset" => 253,
        "ethereal" => 240,
        "vantablack" => 249,
        "white" => 254,
        "starship" => 236,
        "cool-dawn" => 232,
        "catppuccin-latte" | "everforest" | "flexoki-light" | "gruvbox" | "hackerman"
        | "kanagawa" | "lumon" | "lupine" | "miasma" | "nord" | "osaka-jade" | "retro-82"
        | "ristretto" | "rose-pine" | "solitude" | "tokyo-night" => 255,
        _ => panic!("{name}: stock inventory changed, no recorded QML glass floor"),
    }
}

#[test]
fn theme_gate_stock_contrast_floors() {
    let mut failures = Vec::new();
    let mut checks = 0;
    let mut legacy_failures = 0;
    let mut surface_frame_allowances = 0;
    for (name, body) in super::palettes() {
        let (theme, loaded) = Theme::from_text_observed(&body);
        assert!(
            loaded.0,
            "{name}: production palette resolver did not parse"
        );
        let list = Palette::inherited(&TextStyle::default(), theme);
        let card = CardPalette::from_theme(&TextStyle::default(), theme, 8.);
        let mut minima: BTreeMap<&str, f64> = BTreeMap::new();
        let mut floor = |role: &'static str, ink, ground, need: f64| {
            checks += 1;
            let got = contrast(ink, ground);
            minima
                .entry(role)
                .and_modify(|old| *old = old.min(got))
                .or_insert(got);
            // Float conversion may move an exact ratio by a few millionths, never by one 8-bit step.
            if got + 0.00001 < need {
                if role == "secondary_button"
                    && ["catppuccin-latte", "flexoki-light", "lupine", "rose-pine"]
                        .contains(&name.as_str())
                {
                    surface_frame_allowances += 1;
                } else {
                    failures.push(format!("{name} {role}: {got:.6} < {need:.6}"));
                }
            }
        };
        // Theme.qml::sweep, same text and frame thresholds and both error/caption comparisons.
        for ground in [theme.background, theme.surface] {
            floor("foreground", theme.foreground, ground, 4.5);
            floor("muted", card.muted, ground, 4.5);
            floor("metadata", list.metadata, ground, 4.5);
            floor("metadata_glyph", list.metadata_frame, ground, 3.);
            floor(
                "footer_caption",
                crate::g2::fb1::Ink::interim(&list).muted,
                ground,
                4.5,
            );
            if contrast(theme.foreground.opacity(0.62), ground) < 4.5 {
                legacy_failures += 1;
            }
        }
        floor("muted_frame", list.muted_frame, theme.background, 3.);
        let footer = crate::g2::fb1::Ink::interim(&list);
        floor(
            "footer_error",
            footer.error,
            theme.background,
            4.5_f64.max(contrast(footer.muted, theme.background)),
        );
        // Only the four named muted-glyph-on-surface exceptions can waive this unchanged floor.
        floor("secondary_button", card.muted_frame, theme.surface, 3.);
        floor("accent_frame", card.accent_frame, theme.surface, 3.);
        floor("symlink", list.symlink, theme.background, 4.5);
        floor("executable", list.executable, theme.background, 4.5);
        floor(
            "error",
            card.error,
            theme.background,
            4.5_f64.max(contrast(card.muted, theme.background)),
        );
        floor(
            "status_error",
            card.error_on_surface,
            theme.surface,
            4.5_f64.max(contrast(card.muted, theme.surface)),
        );
        floor("copy_progress", card.accent_frame, card.surface, 3.);
        // palette.rs uses 3:1 for graphic ink; OMI-SPINNER.md prescribes the raw accent.
        floor("omi_accent", theme.accent, theme.background, 3.);
        // DESIGN-040 CopyTo/Jobs recipe: the copy track is the muted frame at 25%.
        assert_eq!(flea::ui_logic_op1::card::JOBS_TRACK_ALPHA, 0.25);
        minima.insert(
            "rail_hairline",
            contrast(
                theme.foreground.opacity(crate::list::HAIRLINE),
                theme.background,
            ),
        );
        // DESIGN-040 rule 2: decorative rail hairline is the 12% foreground recipe, not a control boundary.
        assert_eq!(crate::list::HAIRLINE, 0.12);
        // The cursor wash keeps the resting ink, QML cursorRecipe and MenuRow's foreground at 8%.
        assert_eq!(crate::list::HOVER, 0.08);
        let menu = crate::g2::mn1::element::MenuInk::from_palette(&list);
        assert_eq!(menu.wash, theme.foreground.opacity(0.08));
        assert_eq!(menu.rule, list.muted_frame.opacity(0.4));
        assert_eq!(menu.muted, list.muted);
        assert_eq!(menu.error, list.error);
        // The secondary button and resting file glyph must use their own roles, not each other's ink.
        assert_eq!(card.muted_frame, theme.roles.muted_frame);
        assert_eq!(list.metadata_frame, theme.roles.metadata_frame);
        if name == "sunset" {
            assert_ne!(card.muted_frame, card.muted);
        }

        let worst = |alpha: u16| {
            let mut worst = f64::INFINITY;
            for plane in [theme.background, theme.surface] {
                for backdrop in [0., (255_f64 * 0.75).round() / 255.] {
                    let mut plane = plane;
                    plane.a = alpha as f32 / 255.;
                    let ground = over(plane, [backdrop; 3]);
                    for ink in [card.muted, list.metadata] {
                        worst = worst.min(ratio(over(ink, ground), ground));
                    }
                }
            }
            worst
        };
        assert!(worst(255) + 0.00001 >= 4.5, "{name}: opaque glass floor");
        let mut glass_floor = 255;
        while glass_floor > 0 && worst(glass_floor - 1) >= 4.5 {
            glass_floor -= 1;
        }
        assert!(
            glass_floor <= recorded_floor(&name),
            "{name}: glass floor {glass_floor} exceeds QML {}",
            recorded_floor(&name)
        );
        for (role, value) in minima {
            println!("THEME_GATE floor {name} {role} {value:.6}");
        }
        println!("THEME_GATE glass {name} {glass_floor}/255");
        println!(
            "THEME_GATE before {name} metadata {:.6} frame {:.6}",
            [theme.background, theme.surface]
                .into_iter()
                .map(|g| contrast(theme.foreground.opacity(0.62), g))
                .fold(f64::INFINITY, f64::min),
            contrast(theme.foreground.opacity(0.5), theme.background)
        );
    }
    assert_eq!(
        surface_frame_allowances, 4,
        "DESIGN-040 muted glyph on surface allowance count changed"
    );
    assert!(
        legacy_failures > 0,
        "negative control: fixed-opacity metadata unexpectedly meets all floors"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    println!(
        "THEME_GATE CONTRAST checks={checks} legacy_metadata_misses={legacy_failures} failed=0"
    );
}
