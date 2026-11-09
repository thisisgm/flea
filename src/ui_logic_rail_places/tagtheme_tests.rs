// The swatch theme: ported from ui/js/Contrast.js tagTheme with the 0.4.0 fallback triggers (README Tags).
use super::tagtheme::*;

fn palette<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |role| {
        pairs
            .iter()
            .find(|(name, _)| *name == role)
            .map(|(_, hex)| (*hex).to_owned())
    }
}

const DISTINCT: [(&str, &str); 7] = [
    ("red", "#e06c75"),
    ("orange", "#d19a66"),
    ("yellow", "#e5c07b"),
    ("green", "#98c379"),
    ("blue", "#61afef"),
    ("magenta", "#c678dd"),
    ("dark_foreground", "#5c6370"),
];

#[test]
fn a_distinct_palette_draws_swatches_lifted_to_three_to_one() {
    let theme = tag_theme(palette(&DISTINCT), "#282c34", "#21252b");
    assert!(!theme.fallback, "seven distinct colours keep the swatch");
    for (at, hex) in theme.colors.iter().enumerate() {
        assert!(
            ratio_of(parse(hex).unwrap(), parse("#282c34").unwrap()) >= MIN_RATIO,
            "listing colour {at} reaches 3:1: {hex}"
        );
        assert!(
            ratio_of(
                parse(&theme.menu_colors[at]).unwrap(),
                parse("#21252b").unwrap()
            ) >= MIN_RATIO,
            "menu colour {at} reaches 3:1"
        );
    }
    assert_eq!(
        theme.colors[0], "#e06c75",
        "a colour already at 3:1 is kept as given"
    );
}

#[test]
fn the_fallback_stands_only_for_missing_shared_or_grey_colours() {
    let mut missing = DISTINCT.to_vec();
    missing.retain(|(role, _)| *role != "orange");
    let theme = tag_theme(palette(&missing), "#282c34", "#21252b");
    assert!(theme.fallback, "a tag with no colour falls back to digits");
    assert_eq!(theme.colors[1], "", "and its swatch colour stays empty");
    let mut shared = DISTINCT;
    shared[4] = ("blue", "#98C379");
    assert!(
        tag_theme(palette(&shared), "#282c34", "#21252b").fallback,
        "two tags sharing one exact colour fall back, case aside"
    );
    let mut grey = DISTINCT;
    grey[0] = ("red", "#7f7a7a");
    assert!(
        tag_theme(palette(&grey), "#282c34", "#21252b").fallback,
        "a colour tag that is a grey falls back"
    );
    let mut off_hue = DISTINCT;
    off_hue[4] = ("blue", "#4f9a8a");
    assert!(
        !tag_theme(palette(&off_hue), "#282c34", "#21252b").fallback,
        "a hue outside its name's window no longer trips the fallback"
    );
    let mut gray_tag = DISTINCT;
    gray_tag[6] = ("dark_foreground", "#808080");
    assert!(
        !tag_theme(palette(&gray_tag), "#282c34", "#21252b").fallback,
        "the Gray tag may be grey"
    );
    let mut bad = DISTINCT;
    bad[2] = ("yellow", "nope");
    assert!(
        tag_theme(palette(&bad), "#282c34", "#21252b").fallback,
        "an unreadable colour is no colour"
    );
}

#[test]
fn ensure_ratio_walks_toward_the_far_end_and_steps_past_rounding() {
    let dark = "#1d1f21";
    let lifted = ensure_ratio("#303030", dark, 3.0);
    assert!(
        ratio_of(parse(&lifted).unwrap(), parse(dark).unwrap()) >= 3.0,
        "a dim colour lifts on a dark ground: {lifted}"
    );
    let light = "#fafafa";
    let darkened = ensure_ratio("#f0e0a0", light, 3.0);
    assert!(
        ratio_of(parse(&darkened).unwrap(), parse(light).unwrap()) >= 3.0,
        "a pale colour darkens on a light ground: {darkened}"
    );
    assert_eq!(ensure_ratio("#ffffff", "#000000", 3.0), "#ffffff");
    assert_eq!(
        hex_of([1.2, -0.1, 0.5]),
        "#ff0080",
        "channels clamp before they round"
    );
    assert!(is_grey([0.5, 0.5, 0.52]) && !is_grey([0.9, 0.3, 0.3]) && is_grey([0.0, 0.0, 0.0]));
    assert_eq!(
        parse("ff112233"),
        parse("#112233"),
        "an eight-digit colour drops its alpha"
    );
}

#[test]
fn every_lift_holds_three_to_one_after_rounding_on_any_ground() {
    let grounds = [
        "#000000", "#101010", "#1e1e2e", "#2e3440", "#7f7f7f", "#d8dee9", "#eff1f5", "#ffffff",
    ];
    for ground in grounds {
        for (_, hex) in DISTINCT {
            let out = ensure_ratio(hex, ground, 3.0);
            let got = ratio_of(parse(&out).unwrap(), parse(ground).unwrap());
            assert!(
                got >= 3.0 || out == "#000000" || out == "#ffffff",
                "{hex} on {ground} gave {out} at {got}"
            );
        }
    }
}

#[test]
fn the_bench_palette_draws_the_tag_marks_and_tags_menu_board_colours() {
    let bench = [
        ("red", "#d65a4f"),
        ("orange", "#e19763"),
        ("yellow", "#dc9c36"),
        ("green", "#79ab55"),
        ("blue", "#509467"),
        ("magenta", "#8f7351"),
        ("dark_foreground", "#907e64"),
    ];
    let theme = tag_theme(palette(&bench), "#1c120d", "#150d0a");
    let board = [
        "#d65a4f", "#e19763", "#dc9c36", "#79ab55", "#509467", "#8f7351", "#907e64",
    ];
    assert!(
        !theme.fallback,
        "the bench keeps swatches although Purple sits close to Gray"
    );
    assert_eq!(
        theme.colors,
        board.map(String::from),
        "--tag-* on the listing ground"
    );
    assert_eq!(
        theme.menu_colors,
        board.map(String::from),
        "--tagm-* on the menu surface"
    );
}

#[test]
fn a_colour_far_from_its_tags_name_draws_the_canonical_colour() {
    // Omarchy's stock hackerman: red, orange and yellow are all greens.
    let hackerman = [
        ("red", "#50f872"),
        ("orange", "#50f7a3"),
        ("yellow", "#50f7d4"),
        ("green", "#4fe88f"),
        ("blue", "#829dd4"),
        ("magenta", "#86a7df"),
        ("dark_foreground", "#6a6e95"),
    ];
    let theme = tag_theme(palette(&hackerman), "#0b0c16", "#080910");
    let want = [
        "#f7768e", "#eb927b", "#e0af68", "#4fe88f", "#829dd4", "#86a7df", "#6a6e95",
    ];
    assert_eq!(theme.colors, want.map(String::from), "listing");
    assert_eq!(theme.menu_colors, want.map(String::from), "menu");
    assert!(
        !theme.fallback,
        "a canonical colour is not the digit fallback"
    );
    for (at, hex) in theme.colors.iter().take(3).enumerate() {
        let rgb = parse(hex).unwrap();
        assert_eq!(
            outside(hue(rgb), HUE_WINDOWS[at]),
            0.0,
            "{at} {hex} sits in its name's window"
        );
    }
}

#[test]
fn a_hue_near_its_name_and_a_grey_keep_the_themes_colour() {
    // Purple a blue and Blue a teal stay, as do greys, which name no other colour.
    assert_eq!(named(5, "#86a7df"), "#86a7df");
    assert_eq!(named(4, "#509467"), "#509467");
    assert_eq!(named(0, "#565d60"), "#565d60");
    assert_eq!(named(6, "#50f872"), "#50f872", "the Gray tag has no window");
    assert_eq!(named(4, "#e68e0d"), "#7aa2f7", "an orange Blue is far");
    assert_eq!(named(0, "nope"), "nope");
}

#[test]
fn hue_windows_wrap_past_red() {
    assert_eq!(hue([1.0, 0.0, 0.0]), 0.0);
    assert!((hue([1.0, 0.0, 0.5]) - 330.0).abs() < 1e-9);
    assert!((hue([0.0, 0.0, 1.0]) - 240.0).abs() < 1e-9);
    assert_eq!(outside(350.0, (335.0, 12.0)), 0.0);
    assert_eq!(outside(5.0, (335.0, 12.0)), 0.0);
    assert_eq!(outside(132.0, (335.0, 12.0)), 120.0);
    assert_eq!(outside(330.0, (335.0, 12.0)), 5.0);
    assert_eq!(outside(33.0, (250.0, 335.0)), 58.0);
}
