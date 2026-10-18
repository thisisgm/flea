use super::Theme;

#[test]
fn theme_review_r2_control_alpha_matches_number_coercion() {
    for (raw, expected) in [
        (" 0.5 ", 0.5),
        ("0x1", 1.0),
        ("0b1", 1.0),
        ("0o1", 1.0),
        (" ", 0.0),
    ] {
        let t = Theme::resolve(
            "background = '#000000'\nforeground = '#ffffff'",
            &format!("[controls]\nnormal-fill-alpha = '{raw}'"),
            "",
            None,
            "monospace",
        );
        println!(
            "CONTROL_ALPHA input={raw:?} Rust={} Qt={expected}",
            t.numbers["style.normalFillAlpha"]
        );
        assert_eq!(
            t.numbers["style.normalFillAlpha"], expected,
            "input={raw:?}"
        );
    }
}

#[test]
fn theme_review_r2_control_border_widths_are_available() {
    let t = Theme::resolve(
        "background = '#000000'\nforeground = '#ffffff'",
        "[controls]\nnormal-border-width = 2\nhover-cursor-border-width = 3\nselected-border-width = 4\nfocus-border-width = 5",
        "",
        None,
        "monospace",
    );
    for (name, expected) in [
        ("normal", 2.0),
        ("hover", 3.0),
        ("selected", 4.0),
        ("focus", 5.0),
    ] {
        let key = format!("style.{name}BorderWidth");
        println!(
            "CONTROL_WIDTH {key} Rust={:?} Qt={expected}",
            t.numbers.get(&key)
        );
        assert_eq!(t.numbers.get(&key), Some(&expected), "{key} is missing");
    }
}

#[test]
fn theme_review_r2_control_number_forms_and_invalid_fallbacks() {
    use super::{parse::finite, Values};
    for (raw, expected) in [
        ("", Some(0.0)),
        (" \t\r\n", Some(0.0)),
        (" 0.5 ", Some(0.5)),
        ("\u{00a0}+.5e0\u{00a0}", Some(0.5)),
        ("0x1", Some(1.0)),
        ("0Xf", Some(15.0)),
        ("0b10", Some(2.0)),
        ("0B11", Some(3.0)),
        ("0o7", Some(7.0)),
        ("0O10", Some(8.0)),
        ("077", Some(77.0)),
        ("-.5", Some(-0.5)),
        ("-0", Some(-0.0)),
        ("1.e2", Some(100.0)),
        ("1e-999", Some(0.0)),
        ("0x20000000000001", Some(9007199254740992.0)),
        ("0x20000000000003", Some(9007199254740996.0)),
        ("0x7fffffffffffffff", Some(i64::MAX as f64)),
        ("0x-8000000000000000", Some(i64::MIN as f64)),
        ("0x8000000000000000", None),
        ("0x-8000000000000001", None),
        ("0xffffffffffffffff", None),
        ("0x10000000000000000", None),
        (
            "0b10000000000000000000000000000000000000000000000000000000000000000",
            None,
        ),
        ("0o2000000000000000000000", None),
        ("2px", None),
        ("1e", None),
        ("+0x1", None),
        ("-0b1", None),
        ("0x+1", Some(1.0)),
        ("0b+1", Some(1.0)),
        ("0o+1", Some(1.0)),
        ("0x-1", Some(-1.0)),
        ("0b-1", Some(-1.0)),
        ("0o-1", Some(-1.0)),
        ("0x \t1", Some(1.0)),
        ("0x\u{000b}1", Some(1.0)),
        ("0x\u{0085}1", None),
        ("0x\u{00a0}1", None),
        ("0x1 1", None),
        ("0x++1", None),
        ("0x", None),
        ("0b2", None),
        ("0o8", None),
        ("0x1junk", None),
        ("NaN", None),
        ("Infinity", None),
        ("1e999", None),
        ("\u{0085}0.5", Some(0.5)),
        ("\u{feff}0.5", None),
        ("1_0", None),
    ] {
        let values = Values::from([("alpha".to_owned(), raw.to_owned())]);
        assert_eq!(
            finite(&values, "alpha", 7.25),
            expected.unwrap_or(7.25),
            "{raw:?}"
        );
        if raw.is_empty() || raw.contains(['\t', '\r', '\n']) {
            continue;
        }
        let t = control_theme(&format!(
            "[controls]\nnormal-fill-alpha = '{raw}'\nnormal-border-alpha = '{raw}'"
        ));
        assert_eq!(
            t.numbers["style.normalFillAlpha"],
            expected.unwrap_or(0.04).clamp(0.0, 1.0),
            "fill {raw:?}"
        );
        assert_eq!(
            t.fills["style.normalBorderColor"].a,
            expected.unwrap_or(0.4).clamp(0.0, 1.0),
            "frame {raw:?}"
        );
        if t.numbers["style.normalFillAlpha"] == 0.0 {
            assert!(!t.numbers["style.normalFillAlpha"].is_sign_negative());
            assert!(!t.fills["style.normalBorderColor"].a.is_sign_negative());
            assert_eq!(t.tokens()["style.normalFillAlpha"], "0.000000");
        }
    }
    assert_eq!(
        super::parse::finite(&super::Values::new(), "absent", 0.4),
        0.4
    );
}

fn control_theme(shell: &str) -> Theme {
    Theme::resolve(
        "background = '#000000'\nforeground = '#ffffff'",
        shell,
        "",
        None,
        "monospace",
    )
}

#[test]
fn theme_review_r2_width_defaults_rounding_clamp_and_inheritance() {
    for (body, expected) in [
        ("", [1.0, 1.0, 0.0, 1.0]),
        ("normal-border-width = 2.5", [3.0, 3.0, 0.0, 3.0]),
        ("hover-cursor-border-width = 3.5", [1.0, 4.0, 0.0, 4.0]),
        ("selected-border-width = 4.5", [1.0, 1.0, 5.0, 1.0]),
        ("focus-border-width = 5.5", [1.0, 1.0, 0.0, 6.0]),
        ("normal-border-width = -2\nhover-cursor-border-width = -1\nselected-border-width = -0.5\nfocus-border-width = -3", [0.0; 4]),
        ("normal-border-width = 2.5\nhover-cursor-border-width = 'bad'\nselected-border-width = 'Infinity'\nfocus-border-width = 'NaN'", [3.0, 3.0, 0.0, 3.0]),
        ("normal-border-width = 2\nhover-cursor-border-width = 3\nselected-border-width = 4", [2.0, 3.0, 4.0, 3.0]),
        ("normal-border-width = 3\nhover-cursor-border-width = ' '\nfocus-border-width = '0x0'", [3.0, 0.0, 0.0, 0.0]),
        ("normal-border-width = '0x2'\nhover-cursor-border-width = '0b11'\nselected-border-width = '0o4'\nfocus-border-width = '0X5'", [2.0, 3.0, 4.0, 5.0]),
    ] {
        let t = control_theme(&format!("[controls]\n{body}"));
        let tokens = t.tokens();
        for (name, n) in ["normal", "hover", "selected", "focus"].into_iter().zip(expected) {
            let key = format!("style.{name}BorderWidth");
            assert_eq!(t.numbers.get(&key), Some(&n), "{body}/{key}");
            assert_eq!(tokens.get(&key), Some(&format!("{n:.6}")), "{body}/{key}");
        }
    }
}

#[test]
fn theme_review_r2_control_number_and_spacing_prefix_remain_distinct() {
    let t = control_theme(
        "[controls]\nnormal-fill-alpha = '0x1'\n[spacing]\nscale = '0x1'\nrow-gap = '0x2'",
    );
    assert_eq!(t.numbers["style.normalFillAlpha"], 1.0);
    assert_eq!(t.spacing.scale, 0.0);
    assert_eq!(t.spacing.pixels["rowGap"], 0);
    let t = control_theme(
        "[controls]\nnormal-fill-alpha = '2px'\n[spacing]\nscale = '2px'\nrow-gap = '5px'",
    );
    assert_eq!(t.numbers["style.normalFillAlpha"], 0.04);
    assert_eq!(t.spacing.scale, 2.0);
    assert_eq!(t.spacing.pixels["rowGap"], 5);
}
