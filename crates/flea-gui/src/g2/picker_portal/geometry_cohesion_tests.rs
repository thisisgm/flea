//! Real chooser endpoint receipts for painted cohesion controls.
use super::*;
use flea::jsondoc::Json;

#[gpui::test]
fn followup_picker_cohesion_reads_drawn_controls(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    f.cx.run_until_parked();
    let reply = endpoint(&mut f, "cohesionControls", &[]);
    assert_eq!(reply.code, 0, "{reply:?}");
    let state = flea::jsondoc::parse(&reply.stdout).unwrap();
    let controls = state.get("controls").unwrap().as_array().unwrap();
    assert_eq!(
        controls
            .iter()
            .filter(|row| row.get("kind").and_then(Json::as_str) == Some("DialogButton"))
            .count(),
        2
    );
    assert!(controls
        .iter()
        .any(|row| row.get("kind").and_then(Json::as_str) == Some("ChromeButton")));
    for row in controls {
        for key in ["exposed", "targetX", "targetY", "hoverLive", "pressLive"] {
            assert!(row.get(key).is_some(), "missing {key}: {row:?}");
        }
    }
}

#[gpui::test]
fn followup_picker_cohesion_reports_the_answer_pressed_state(cx: &mut TestAppContext) {
    let mut f = fixture(cx, "{}", None);
    let reply = endpoint(&mut f, "controls", &[]);
    let controls = flea::jsondoc::parse(&reply.stdout).unwrap();
    let bounds = controls.as_array().unwrap()[1]
        .get("bounds")
        .unwrap()
        .as_array()
        .unwrap();
    let at = gpui::point(
        gpui::px((bounds[0].as_f64().unwrap() + bounds[2].as_f64().unwrap() / 2.) as f32),
        gpui::px((bounds[1].as_f64().unwrap() + bounds[3].as_f64().unwrap() / 2.) as f32),
    );
    f.cx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    let reply = endpoint(&mut f, "cohesionControls", &[]);
    let state = flea::jsondoc::parse(&reply.stdout).unwrap();
    let buttons = state.get("controls").unwrap().as_array().unwrap();
    assert_eq!(buttons[1].get("pressed").unwrap().as_bool(), Some(true));
    assert_eq!(buttons[1].get("enabled").unwrap().as_bool(), Some(false));
    f.cx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::default());
    let reply = endpoint(&mut f, "cohesionControls", &[]);
    let state = flea::jsondoc::parse(&reply.stdout).unwrap();
    assert_eq!(
        state.get("controls").unwrap().as_array().unwrap()[1]
            .get("pressed")
            .unwrap()
            .as_bool(),
        Some(false)
    );
}
