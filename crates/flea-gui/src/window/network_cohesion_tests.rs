//! Text fields own selection while buttons and chips own held presses.
use super::*;

#[gpui::test]
fn cohesion_network_press_capability_follows_the_painted_control(cx: &mut TestAppContext) {
    let (shell, cx, _) = open(Rc::new(Recorder::default()), cx);
    cx.update(|window, cx| shell.update(cx, |shell, cx| shell.network_add(0, window, cx)));
    frame(cx);
    let state = flea::jsondoc::parse(&call(&shell, "cohesionControls", &[], cx)).unwrap();
    let controls = state.get("controls").unwrap().as_array().unwrap();
    assert!(controls.iter().any(|row| {
        row.get("kind").and_then(flea::jsondoc::Json::as_str) == Some("DialogField")
    }));
    for control in controls {
        let text = control.get("kind").and_then(flea::jsondoc::Json::as_str) == Some("DialogField");
        assert_eq!(
            control
                .get("pressLive")
                .and_then(flea::jsondoc::Json::as_bool),
            Some(!text),
            "{control:?}"
        );
    }
}
