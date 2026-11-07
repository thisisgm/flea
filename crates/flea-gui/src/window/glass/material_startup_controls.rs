//! The GPUI port of tests/material-startup.sh: each startup mode paints its ground alpha, and only automatic Glassy searches for it.
use super::*;
use crate::window::glass::Inputs;

// The frozen QML oracle's fixture theme and its expected 8-bit alphas, leg for leg.
const COLORS: &str = "background = \"#1e2030\"\ndark_background = \"#2c3045\"\nforeground = \"#d0d4e0\"\naccent = \"#7aa2f7\"\n";
const OVERRIDE: &str = "[flea]\nbackground-alpha = 0.34\n";

struct Leg {
    name: &'static str,
    files: &'static [(usize, &'static str)],
    alpha: u8,
    searches: bool,
}

const LEGS: [Leg; 5] = [
    Leg {
        name: "opaque",
        files: &[(0, COLORS), (5, "return\n")],
        alpha: 255,
        searches: false,
    },
    Leg {
        name: "transparent",
        files: &[(0, COLORS)],
        alpha: 255,
        searches: false,
    },
    Leg {
        name: "user-override",
        files: &[(0, COLORS), (6, "return\n"), (2, OVERRIDE)],
        alpha: 87,
        searches: false,
    },
    Leg {
        name: "theme-override",
        files: &[(0, COLORS), (6, "return\n"), (1, OVERRIDE)],
        alpha: 87,
        searches: false,
    },
    Leg {
        name: "glassy",
        files: &[(0, COLORS), (6, "return\n")],
        alpha: 192,
        searches: true,
    },
];

/// The alpha the mounted window's producers actually painted, as 8 bits; `alpha` in the glass reply is that sample, never the request.
fn painted(value: &Json) -> u8 {
    let alpha = value.get("alpha").and_then(Json::as_f64).expect("alpha");
    assert!((0. ..=1.).contains(&alpha), "{value:?}");
    (alpha * 255.).round() as u8
}

#[gpui::test]
fn material_startup_each_mode_paints_its_alpha_and_only_glassy_searches(cx: &mut TestAppContext) {
    for leg in &LEGS {
        Inputs::take_floor_searches();
        let fixture = install(cx, &format!("material-startup-{}", leg.name), leg.files);
        let handle = launch(cx);
        draw(handle, cx);
        let value = glass_value(handle, cx);
        let searches = Inputs::take_floor_searches();
        let elapsed: std::time::Duration = searches.iter().sum();
        println!(
            "MATERIAL_STARTUP {} calls={} floorUs={} alpha={} expected={}",
            leg.name,
            searches.len(),
            elapsed.as_micros(),
            painted(&value),
            leg.alpha
        );
        assert_eq!(
            value.get("groundReady").and_then(Json::as_bool),
            Some(true),
            "{}: the producers never reported a painted ground: {value:?}",
            leg.name
        );
        assert_eq!(painted(&value), leg.alpha, "{}: {value:?}", leg.name);
        assert_eq!(
            !searches.is_empty(),
            leg.searches,
            "{}: {} Glassy alpha searches at startup",
            leg.name,
            searches.len()
        );
        stop(handle, cx);
        fixture.cleanup();
    }
}
