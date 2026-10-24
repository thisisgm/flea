use super::*;
use crate::ui_logic_pv2::code::{gutter_ink, inks, role_name, FALLBACK};

const DARK: &str = "#101315";
const SURFACE: &str = "#181825";

fn none(_: &str) -> Option<&'static str> {
    None
}

// Ported from tests/js/codelines.js: the inks, each lifted to 4.5 on whichever of the two grounds it contrasts less with.
#[test]
fn code_inks_without_a_palette_are_the_foreground_then_the_fallbacks() {
    let out = inks(none, DARK, SURFACE, "#cccccc");
    let want: Vec<String> = std::iter::once("#cccccc")
        .chain(FALLBACK)
        .map(String::from)
        .collect();
    assert_eq!(
        out.to_vec(),
        want,
        "no palette gives the foreground then the six fallbacks"
    );
}

#[test]
fn code_inks_keep_a_passing_palette_colour_and_its_ansi_slot() {
    let blue = inks(
        |key| (key == "blue").then_some("#5599ff"),
        DARK,
        SURFACE,
        "#cccccc",
    );
    assert_eq!(
        blue[4], "#5599ff",
        "a palette colour passing on both grounds is kept"
    );
    let slot = inks(
        |key| (key == "color5").then_some("#ccaaff"),
        DARK,
        SURFACE,
        "#cccccc",
    );
    assert_eq!(slot[5], "#ccaaff", "an unnamed colour takes its ANSI slot");
}

#[test]
fn code_inks_lift_to_four_and_a_half() {
    let light = |key: &str| match key {
        "background" => Some("#f4f4ef"),
        "dark_background" => Some("#e4e4df"),
        "yellow" => Some("#f0f0d0"),
        _ => None,
    };
    let pale = &inks(light, DARK, SURFACE, "#222222")[3];
    assert!(
        ratio(pale, "#f4f4ef") >= 4.5,
        "a yellow too pale for a light ground is lifted to 4.5"
    );
    let mid = |key: &str| match key {
        "background" => Some("#303030"),
        "dark_background" => Some("#404040"),
        "red" => Some("#884444"),
        _ => None,
    };
    let red = &inks(mid, DARK, SURFACE, "#cccccc")[1];
    assert!(
        ratio(red, "#404040") >= 4.5,
        "a red under 4.5 on the surface is lifted against it, the less contrasting ground"
    );
    for i in 1..=6 {
        let ink = FALLBACK[i - 1];
        let worst = ratio(ink, DARK).min(ratio(ink, SURFACE));
        assert!(
            worst >= 4.5,
            "fallback {} holds 4.5 on both dark grounds",
            role_name(i as i64)
        );
    }
}

// ql-code CHANGES: line numbers draw in the muted ink lifted to 4.5:1, where v0.3.12 drew them at 2.29:1.
#[test]
fn code_gutter_ink_is_muted_lifted_to_four_and_a_half() {
    let board_muted = "#8b7b74";
    let (bg, surface) = ("#1c120d", "#150d0a");
    let ink = gutter_ink(board_muted, bg, surface);
    assert!(
        ratio(&ink, bg) >= 4.5 && ratio(&ink, surface) >= 4.5,
        "the gutter holds 4.5 on both grounds: {ink}"
    );
    // v0.3.12 drew the muted ink at opacity 0.6 over the ground: #8b7b74 at 60% on #1c120d is #5f514b.
    let v0312 = "#5f514b";
    assert!(
        ratio(v0312, bg) < 2.5,
        "the oracle's dimmed number sat near 2.29:1"
    );
    assert_eq!(
        gutter_ink("#ffffff", bg, surface),
        "#ffffff",
        "an ink that already holds is kept as given"
    );
}

#[test]
fn contrast_parse_and_round_trip() {
    assert_eq!(parse("#ff0000"), Some([1., 0., 0.]));
    assert_eq!(
        parse("#80ff0000"),
        Some([1., 0., 0.]),
        "an alpha-first hex drops its alpha"
    );
    assert_eq!(parse("#xyz"), None, "a malformed hex is refused");
    assert_eq!(hex([1., 0.5, 0.]), "#ff8000");
    assert!(
        (ratio("#000000", "#ffffff") - 21.).abs() < 1e-9,
        "black on white is 21:1"
    );
    let lifted = ensure_ratio("#777777", "#808080", 3.);
    assert!(
        ratio(&lifted, "#808080") >= 3.,
        "the stepped walk lands on the asked ratio"
    );
}
