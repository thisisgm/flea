//! The visual registry: every extra is a drivable state, not a name alone.
use flea::jsondoc::{self, Json};

const REGISTRY: &str = include_str!("../../../../../tests/visual/g2/op2.json");

fn scenes(manifest: &Json) -> &[Json] {
    manifest
        .get("scenes")
        .and_then(Json::as_array)
        .expect("scenes")
}

fn name(scene: &Json) -> &str {
    scene
        .get("scene")
        .and_then(Json::as_str)
        .expect("scene name")
}

fn is_extra(scene: &Json) -> bool {
    name(scene).starts_with("g2.op2.extra.")
}

#[test]
fn every_extra_scene_is_a_drivable_state() {
    let manifest = jsondoc::parse(REGISTRY).expect("the op2 visual registry parses");
    let scenes = scenes(&manifest);
    let extras: Vec<&Json> = scenes.iter().filter(|scene| is_extra(scene)).collect();
    assert_eq!(
        (scenes.len(), extras.len()),
        (53, 49),
        "4 primaries and 49 extras are registered"
    );
    for scene in extras {
        let scene_name = name(scene);
        for key in [
            "title",
            "board_png",
            "board_html",
            "before_png",
            "sha256",
            "fixture_variant",
            "language",
            "size",
            "scale",
            "text_size",
            "rounding",
            "rounding_variants",
            "parity_ids",
            "producer_owner",
            "setup",
            "assertions",
        ] {
            assert!(scene.get(key).is_some(), "{scene_name} carries {key}");
        }
        assert_eq!(scene.get("unit").and_then(Json::as_str), Some("g2-op2"));
        assert_eq!(
            scene.get("language").and_then(Json::as_str),
            Some("en"),
            "{scene_name} names its language"
        );
        assert_eq!(
            scene.get("text_size").and_then(Json::as_f64),
            Some(14.),
            "{scene_name} names its text size"
        );
        assert_eq!(
            scene.get("rounding").and_then(Json::as_f64),
            Some(8.),
            "{scene_name} names its rounding"
        );
        let size = scene.get("size").and_then(Json::as_array).expect("size");
        assert_eq!(size.len(), 2, "{scene_name} names its window size");
        let variants = scene
            .get("rounding_variants")
            .and_then(Json::as_array)
            .expect("rounding variants");
        assert!(
            variants.iter().any(|v| v.as_f64() == Some(0.))
                && variants.iter().any(|v| v.as_f64() == Some(8.)),
            "{scene_name} exercises both radii"
        );
        let setup = scene.get("setup").and_then(Json::as_object).expect("setup");
        assert!(
            setup
                .iter()
                .any(|(key, _)| ["card", "view", "job", "phase"].contains(&key.as_str())),
            "{scene_name} names the card, view, job or phase it drives"
        );
        if let Some(blocked) = scene.get("blocked").and_then(Json::as_str) {
            assert!(!blocked.is_empty(), "{scene_name} names why it is blocked");
        }
    }
}

#[test]
fn every_primary_scene_keeps_its_setup_and_board() {
    let manifest = jsondoc::parse(REGISTRY).expect("the op2 visual registry parses");
    let primaries: Vec<&Json> = scenes(&manifest)
        .iter()
        .filter(|scene| name(scene).ends_with(".primary"))
        .collect();
    assert_eq!(primaries.len(), 4, "the four boards are registered");
    for scene in primaries {
        let scene_name = name(scene);
        assert!(
            scene.get("setup").and_then(Json::as_object).is_some(),
            "{scene_name} names its setup"
        );
        assert!(
            scene.get("board_png").and_then(Json::as_str).is_some(),
            "{scene_name} names its board"
        );
        assert!(
            scene.get("sha256").and_then(Json::as_object).is_some(),
            "{scene_name} carries board hashes"
        );
    }
}

/// A setup `press` names a control the card draws; a keystroke goes in `key`, so one word never means both.
const PRESS_TARGETS: [&str; 4] = ["action", "cancel", "compress", "extract"];
const KEYS: [&str; 4] = ["enter", "escape", "space", "tab"];

#[test]
fn every_press_names_a_control_and_every_key_a_keystroke() {
    let manifest = jsondoc::parse(REGISTRY).expect("the op2 visual registry parses");
    for scene in scenes(&manifest) {
        let scene_name = name(scene);
        let setup = scene.get("setup").and_then(Json::as_object).expect("setup");
        for (key, value) in setup.iter() {
            let value = value.as_str();
            match key.as_str() {
                "press" => assert!(
                    value.is_some_and(|target| PRESS_TARGETS.contains(&target)),
                    "{scene_name} presses an unknown control {value:?}"
                ),
                "key" => assert!(
                    value.is_some_and(|stroke| KEYS.contains(&stroke)),
                    "{scene_name} sends an unknown keystroke {value:?}"
                ),
                _ => {}
            }
        }
    }
}

/// Board refs are relative to the design-hig root that `flea-ci-visual --design` names, as mn2.json and op3.json are.
#[test]
fn every_board_ref_is_relative_to_the_design_root() {
    assert!(
        !REGISTRY.contains("/home/"),
        "no operator path in a committed manifest"
    );
    let manifest = jsondoc::parse(REGISTRY).expect("the op2 visual registry parses");
    for scene in scenes(&manifest) {
        let scene_name = name(scene);
        for key in ["board_png", "board_html", "before_png"] {
            let path = scene.get(key).and_then(Json::as_str).unwrap_or("");
            assert!(
                (path.starts_with("boards/") || path.starts_with("before/"))
                    && !path.contains(".."),
                "{scene_name} {key} is relative to the design root: {path:?}"
            );
            let pinned = scene
                .get("sha256")
                .and_then(|pins| pins.get(key))
                .and_then(Json::as_str)
                .unwrap_or("");
            assert_eq!(pinned.len(), 64, "{scene_name} pins {key}");
        }
    }
}
