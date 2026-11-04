use super::{
    contract::{density, Live, Spec},
    geometry::Probe,
};
use flea::jsondoc::Json;

#[test]
fn every_registered_scene_applies_fixed_capture_settings() {
    for view in ["columns", "list", "grid", "dual"] {
        for radius in [0, 8] {
            let spec = Spec::parse(
                &std::process::id().to_string(),
                &format!("g1-01-{view}-r{radius}"),
                r#"{"path":"/fixture"}"#,
            )
            .unwrap();
            assert_eq!((spec.text_size, spec.reduced_motion), (14, true));
            let live = Live {
                scale: 1.,
                text_size: 16.,
                row_height: 31.,
                reduced_motion: false,
                cursor: 3,
                selection: vec![1],
                // A window running a catalog still gives a scene without a language its old receipt.
                language: "ja".into(),
                ..Live::default()
            };
            let receipt = spec.receipt(7, &live);
            assert_eq!(receipt.get("view").and_then(Json::as_str), Some(view));
            assert_eq!(
                receipt.get("corner_radius").and_then(Json::as_f64),
                Some(radius as f64)
            );
            assert_eq!(receipt.get("frame").and_then(Json::as_f64), Some(7.));
            // The receipt states what the window drew, not what the manifest asked for.
            assert_eq!(receipt.get("text_size").and_then(Json::as_f64), Some(16.));
            assert_eq!(receipt.get("reduced_motion"), Some(&Json::Bool(false)));
            assert_eq!(receipt.get("cursor").and_then(Json::as_f64), Some(3.));
            assert_eq!(
                receipt.get("selection"),
                Some(&Json::Arr(vec![Json::Num("1".into())]))
            );
            assert_eq!(receipt.get("language"), None);
        }
    }
}

fn language_scene(language: &str, surface: &str, state: &str) -> Result<Spec, &'static str> {
    let scene = flea::jsondoc::parse(&SCENE.replace(
        '}',
        &format!(r#","language":{language},"surface":{surface},"state":{state}}}"#),
    ))
    .unwrap();
    let settings = flea::jsondoc::parse(r#"{"path":"/fixture"}"#).unwrap();
    Spec::from_scene("i50.fit.de.list.rest.gm.r0", &scene, &settings)
}

#[test]
fn a_language_scene_names_its_language_surface_and_state_in_the_receipt() {
    let spec = language_scene(r#""de""#, r#""list""#, r#""rest""#).unwrap();
    let language = spec.language.as_ref().unwrap();
    assert_eq!(
        (language.code.as_str(), language.surface.as_str()),
        ("de", "list")
    );
    // The window ran ja, so the receipt names ja: it reports the drawn catalog, never the manifest's code.
    let live = Live {
        language: "ja".into(),
        locale_revision: 4,
        ..Live::default()
    };
    let receipt = spec.receipt(9, &live);
    assert_eq!(receipt.get("language").and_then(Json::as_str), Some("ja"));
    assert_eq!(
        receipt.get("locale_revision").and_then(Json::as_f64),
        Some(4.)
    );
    assert_eq!(receipt.get("surface").and_then(Json::as_str), Some("list"));
    assert_eq!(receipt.get("state").and_then(Json::as_str), Some("rest"));
    for code in [r#""zh-CN""#, r#""fil""#] {
        assert!(
            language_scene(code, r#""list""#, r#""rest""#).is_ok(),
            "{code}"
        );
    }
}

#[test]
fn a_language_scene_this_window_cannot_draw_is_refused() {
    for (language, surface, state) in [
        (r#""de""#, r#""menu""#, r#""rest""#),
        (r#""de""#, r#""list""#, r#""hover""#),
        (r#""DE""#, r#""list""#, r#""rest""#),
        (r#""de-""#, r#""list""#, r#""rest""#),
        (r#""zh-CNX""#, r#""list""#, r#""rest""#),
        (r#""d""#, r#""list""#, r#""rest""#),
        ("1", r#""list""#, r#""rest""#),
        (r#""de""#, "null", r#""rest""#),
    ] {
        assert_eq!(
            language_scene(language, surface, state).err(),
            Some("visual.settings"),
            "{language} {surface} {state}"
        );
    }
    for (key, value) in [
        ("language", r#""de""#),
        ("surface", r#""list""#),
        ("state", r#""rest""#),
    ] {
        assert_eq!(
            scene_with(key, value).err(),
            Some("visual.settings"),
            "{key} alone"
        );
    }
}

#[test]
fn wrong_window_unknown_scene_and_invalid_settings_are_refused() {
    let pid = std::process::id().to_string();
    assert_eq!(
        Spec::parse("0", "g1-01-list-r0", "{}").err(),
        Some("visual.window")
    );
    assert_eq!(
        Spec::parse(&pid, "unknown", "{}").err(),
        Some("visual.unknown")
    );
    for settings in [
        "{}",
        r#"{"path":"relative"}"#,
        r#"{"path":"/fixture","width":0}"#,
        r#"{"path":"/fixture","width":"bad"}"#,
        r#"{"path":"/fixture","selection":[1]}"#,
        r#"{"path":"/fixture","text_size":14}"#,
    ] {
        assert_eq!(
            Spec::parse(&pid, "g1-01-list-r0", settings).err(),
            Some("visual.settings")
        );
    }
    assert!(Probe::default().empty());
}

#[test]
fn the_receipt_density_follows_the_drawn_row_height() {
    assert_eq!(density(31.), "compact");
    assert_eq!(density(37.), "unknown");
}

const SCENE: &str = r#"{"view":"list","corner_radius":0,"density":"compact","caret_blink":false,"reduced_motion":true,"cursor":0,"selection":[],"text_size":14}"#;

fn scene_with(key: &str, value: &str) -> Result<Spec, &'static str> {
    let mut scene = flea::jsondoc::parse(SCENE).unwrap();
    if let Json::Obj(pairs) = &mut scene {
        pairs.retain(|(name, _)| name != key);
        pairs.push((key.into(), flea::jsondoc::parse(value).unwrap()));
    }
    let settings = flea::jsondoc::parse(r#"{"path":"/fixture"}"#).unwrap();
    Spec::from_scene("g1-01-list-r0", &scene, &settings)
}

#[test]
fn manifest_selection_cursor_and_text_size_reach_their_value_checks() {
    let spec = scene_with("selection", "[2, 4]").unwrap();
    assert_eq!(
        (spec.cursor, spec.selection, spec.text_size),
        (0, vec![2, 4], 14)
    );
    assert_eq!(scene_with("cursor", "3").unwrap().cursor, 3);
    for size in ["8", "16", "32"] {
        assert!(scene_with("text_size", size).is_ok(), "{size}");
    }
    for (key, value) in [
        ("selection", "[-1]"),
        ("selection", "[1.5]"),
        ("selection", r#"["1"]"#),
        ("selection", "1"),
        ("cursor", "-1"),
        ("cursor", "0.5"),
        ("text_size", "7"),
        ("text_size", "33"),
        ("text_size", "14.5"),
        ("reduced_motion", "1"),
        ("density", r#""comfortable""#),
        ("caret_blink", "true"),
        ("view", r#""tree""#),
    ] {
        assert_eq!(
            scene_with(key, value).err(),
            Some("visual.settings"),
            "{key}={value}"
        );
    }
}
