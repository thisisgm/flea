use super::Theme;
use flea::ui_logic_rail_places::tagtheme::{
    ensure_ratio, hue, is_grey, outside, ratio_of, FAR_FROM_NAME, HUE_WINDOWS, MIN_RATIO, ROLES,
};
use gpui::{Hsla, Rgba};

// Every frozen stock palette plus the two fixture themes, so no installed Omarchy changes the inputs.
fn palettes() -> Vec<(String, String)> {
    let doc = flea::jsondoc::parse(include_str!(
        "../../../../tests/fixtures/theme-palettes.json"
    ))
    .expect("frozen palettes");
    let mut out: Vec<(String, String)> = doc
        .get("palettes")
        .and_then(|p| p.as_object())
        .expect("palettes object")
        .iter()
        .map(|(name, text)| {
            (
                name.clone(),
                text.as_str().expect("palette text").to_owned(),
            )
        })
        .collect();
    assert!(out.len() >= 23, "stock palette coverage shrank");
    out.push((
        "sunset".into(),
        include_str!("../../../../tests/fixtures/sunset/colors.toml").into(),
    ));
    out.push((
        "cool-dawn".into(),
        include_str!("../../../../tests/fixtures/cool-dawn/colors.toml").into(),
    ));
    out
}

fn rgb(color: Hsla) -> [f64; 3] {
    let c: Rgba = color.into();
    [c.r, c.g, c.b].map(|v| f64::from((v.clamp(0., 1.) * 255.).round()) / 255.)
}

fn hex(color: Hsla) -> String {
    flea::ui_logic_rail_places::tagtheme::hex_of(rgb(color))
}

fn role(text: &str, key: &str) -> Option<String> {
    flea::colours::entries(text)
        .into_iter()
        .rev()
        .find(|(name, _)| *name == key)
        .map(|(_, hex)| hex.to_ascii_lowercase())
}

#[test]
fn every_stock_tag_draws_near_its_names_hue_at_three_to_one() {
    for (name, text) in palettes() {
        let theme = Theme::from_text(&text);
        let (ground, plane) = (rgb(theme.background), rgb(theme.surface));
        for (at, key) in ROLES.iter().enumerate() {
            if role(&text, key).is_none() {
                continue;
            }
            for (drawn, under, what) in [
                (theme.tag_listing_inks[at], ground, "listing"),
                (theme.tag_menu_inks[at], plane, "menu"),
            ] {
                let ink = rgb(drawn);
                let held = ratio_of(ink, under) >= MIN_RATIO || ink == [0.0; 3] || ink == [1.0; 3];
                assert!(held, "{name} {key} {what} {} under 3:1", hex(drawn));
                if at < 6 && !is_grey(ink) {
                    let off = outside(hue(ink), HUE_WINDOWS[at]);
                    assert!(
                        off <= FAR_FROM_NAME,
                        "{name} {key} {what} {} sits {off} degrees outside its name's window",
                        hex(drawn)
                    );
                }
            }
        }
    }
}

#[test]
fn hackermans_red_orange_and_yellow_read_as_their_names() {
    let (_, text) = palettes()
        .into_iter()
        .find(|(name, _)| name == "hackerman")
        .expect("hackerman is stock");
    let theme = Theme::from_text(&text);
    for at in 0..3 {
        for ink in [theme.tag_listing_inks[at], theme.tag_menu_inks[at]] {
            assert_eq!(
                outside(hue(rgb(ink)), HUE_WINDOWS[at]),
                0.0,
                "{} draws {}",
                ROLES[at],
                hex(ink)
            );
        }
    }
}

// A palette whose every slot sits near its name keeps every tag colour it drew before the hue rule.
#[test]
fn tokyo_night_and_sunset_keep_their_own_tag_colours() {
    for (name, text) in palettes()
        .into_iter()
        .filter(|(name, _)| name == "tokyo-night" || name == "sunset")
    {
        let theme = Theme::from_text(&text);
        let (ground, plane) = (hex(theme.background), hex(theme.surface));
        for (at, key) in ROLES.iter().enumerate() {
            let own = role(&text, key).expect("every tag role is set");
            assert_eq!(
                hex(theme.tag_listing_inks[at]),
                ensure_ratio(&own, &ground, MIN_RATIO),
                "{name} {key}"
            );
            assert_eq!(
                hex(theme.tag_menu_inks[at]),
                ensure_ratio(&own, &plane, MIN_RATIO),
                "{name} {key} menu"
            );
        }
    }
}
