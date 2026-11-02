//! CTL2 native comparison: the header shares the listing ground and its exact legacy alpha.
use super::*;

const LATTE: &str = r##"mode = "light"
accent = "#1e66f5"
selection = "#ccd0da"
muted = "#acb0be"
background = "#eff1f5"
dark_background = "#e3e4e8"
darker_background = "#d7d8dc"
lighter_background = "#dce0e8"
foreground = "#4c4f69"
dark_foreground = "#9ca0b0"
light_foreground = "#5c5f77"
bright_foreground = "#4c4f69"
red = "#d20f39"
yellow = "#df8e1d"
orange = "#d84e2b"
green = "#40a02b"
cyan = "#179299"
blue = "#1e66f5"
magenta = "#ea76cb"
brown = "#6c2715"
bright_red = "#d20f39"
bright_yellow = "#df8e1d"
bright_green = "#40a02b"
bright_cyan = "#179299"
bright_blue = "#1e66f5"
bright_magenta = "#ea76cb"
"##;

fn latte(cx: &mut TestAppContext, name: &str) -> Json {
    let fixture = install(cx, name, &[(6, "")]);
    fixture.write(&fixture.path().join("input-0"), LATTE);
    let paths: Vec<_> = (0..7)
        .map(|at| fixture.path().join(format!("input-{at}")))
        .collect();
    let snapshot =
        crate::presentation::Snapshot::glass_fixture(&paths, fixture.path().join("font-cache"));
    cx.update(|cx| snapshot.install(cx));
    let handle = launch(cx);
    draw(handle, cx);
    let value = glass_value(handle, cx);
    stop(handle, cx);
    fixture.cleanup();
    value
}

#[gpui::test]
fn sol_glassy_ground_visual_header_uses_the_listing_ground(cx: &mut TestAppContext) {
    let value = latte(cx, "glassy-visual-header");
    let grounds = value.get("grounds").unwrap();
    assert_eq!(grounds.get("header"), grounds.get("listing"), "{value:?}");
    assert_eq!(grounds.get("header").and_then(Json::as_f64), Some(242.));
}

#[gpui::test]
fn sol_glassy_ground_visual_listing_honours_v0312_list_alpha(cx: &mut TestAppContext) {
    let value = latte(cx, "glassy-visual-listing");
    let requested = (value
        .get("requestedListAlpha")
        .and_then(Json::as_f64)
        .unwrap()
        * 255.)
        .round();
    let measured = (value.get("listAlpha").and_then(Json::as_f64).unwrap() * 255.).round();
    assert_eq!(
        requested, 242.,
        "the native v0.3.12 oracle for this exact palette"
    );
    assert_eq!(measured, requested, "{value:?}");
    assert_eq!(
        value
            .get("grounds")
            .unwrap()
            .get("listing")
            .and_then(Json::as_f64),
        Some(measured)
    );
}
