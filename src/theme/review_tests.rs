use super::{Color, Theme};

#[test]
fn theme_review_qcolor_hue_floor_boundary() {
    let t = Theme::resolve(
        "background = '#000000'\nforeground = '#ffffff'\naccent = '#c8a0a0'\nred = '#c8a0a0'",
        "",
        "",
        None,
        "monospace",
    );
    assert!(
        t.colors.accentHasHue,
        "QColor.hsvSaturation is 0.20000000298023224"
    );
}

#[test]
fn theme_review_ansi_bright_rechecks_actual_background() {
    let t = Theme::resolve(
        "color0 = '#ffffff'\ncolor7 = '#000000'\ncolor15 = '#aaaaaa'",
        "",
        "",
        None,
        "monospace",
    );
    assert_eq!(t.colors.opaqueForegroundBright.hex(), "#aaaaaa");
    assert_eq!(t.colors.foregroundBright.hex(), "#000000");
}

#[test]
fn theme_review_spacing_uses_commons_parse_float() {
    let t = Theme::resolve(
        "background = '#000000'\nforeground = '#ffffff'",
        "[spacing]\nscale = '2px'\nrow-gap = '5px'",
        "",
        None,
        "monospace",
    );
    assert_eq!(t.spacing.scale, 2.0);
    assert_eq!(t.spacing.pixels["rowGap"], 5);
    assert_eq!(t.spacing.pixels["rowPaddingX"], 24);
}

#[test]
fn theme_review_qcolor_urgent_boundary_keeps_chroma() {
    let t = Theme::resolve(
        "background = '#000000'\nforeground = '#ffffff'\nred = '#c8a0a0'",
        "",
        "",
        None,
        "monospace",
    );
    assert_ne!(
        t.colors.error,
        Color::WHITE,
        "QML keeps urgent chroma at this saturation"
    );
    assert_eq!(t.colors.error.hex(), "#d1afaf");
}

#[test]
fn theme_review_spacing_prefix_forms_and_control_alpha() {
    for (raw, scale, gap, padding) in [
        ("+2.5em", 2.5, 3, 30),
        ("-2px", 1.0, 8, 12),
        (".5px", 0.5, 1, 6),
        ("2e+2px", 200.0, 200, 2400),
        ("2E-1px", 0.2, 0, 2),
        ("5.e-1px", 0.5, 1, 6),
        ("2e+oops", 2.0, 2, 24),
        ("2Eoops", 2.0, 2, 24),
        ("-.5px", 1.0, 8, 12),
        ("0x10", 0.0, 0, 0),
        ("NaN", 1.0, 8, 12),
        ("Infinity", 1.0, 8, 12),
        ("1e999", 1.0, 8, 12),
        ("++2", 1.0, 8, 12),
        ("  +2px", 2.0, 2, 24),
        ("\u{feff}+2px", 2.0, 2, 24),
        ("\u{0085}+2px", 1.0, 8, 12),
    ] {
        let t = Theme::resolve(
            "background = '#000000'\nforeground = '#ffffff'",
            &format!("[spacing]\nscale = '{raw}'\nrow-gap = '{raw}'"),
            "[controls]\nnormal-fill-alpha = '2px'",
            None,
            "monospace",
        );
        assert_eq!(t.spacing.scale, scale, "scale: {raw}");
        assert_eq!(t.spacing.pixels["rowGap"], gap, "gap: {raw}");
        assert_eq!(t.spacing.pixels["rowPaddingX"], padding, "padding: {raw}");
        assert_eq!(t.numbers["style.normalFillAlpha"], 0.04);
    }
}

#[test]
fn theme_review_qcolor_saturation_keeps_tag_rule_raw() {
    for (hex, expected) in [
        ("#060505", 0.16665904223918915),
        ("#0a0909", 0.09999237209558487),
        ("#c8a0a0", 0.20000000298023224),
        ("#c8b0b0", 0.11999695003032684),
    ] {
        assert_eq!(Color::parse(hex).unwrap().qcolor_saturation(), expected);
    }
    let t = Theme::resolve(
        "background = '#000000'\nforeground = '#ffffff'\nred = '#c8b0b0'\norange = '#ff8000'\nyellow = '#ffff00'\ngreen = '#00ff00'\nblue = '#0000ff'\nmagenta = '#ff00ff'\ndark_foreground = '#777777'",
        "",
        "",
        None,
        "monospace",
    );
    assert!(t.palette["red"].saturation() >= 0.12);
    assert!(t.palette["red"].qcolor_saturation() < 0.12);
    assert!(!t.tags.fallback);
}
