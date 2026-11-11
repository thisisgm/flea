//! Open With model and measured-control reader regressions.
use super::*;

fn assert_controls(state: &Json, model: &str) {
    let expected = jsondoc::parse(&format!("{model}\"tail\":null}}")).unwrap();
    let observed = state.get("controls").and_then(Json::as_array).unwrap();
    let expected = expected.get("controls").and_then(Json::as_array).unwrap();
    assert_eq!(observed.len(), expected.len());
    for (observed, expected) in observed.iter().zip(expected) {
        for key in ["name", "focused", "enabled"] {
            assert_eq!(observed.get(key), expected.get(key), "{key}");
        }
    }
}

#[gpui::test]
fn ipc_readers3_open_with_card_reports_its_model_and_its_painted_parts(cx: &mut TestAppContext) {
    let handle = open(cx);
    value(cx, handle, "openWithState", &[], "{\"opened\":false}");
    update(cx, handle, |shell, _, _| {
        let card = OpenWith::open(1, "photo.png");
        shell.open_with = Some(OpenWithOpen {
            side: 0,
            id: 1,
            card,
            frame: Default::default(),
            initial: Default::default(),
        });
    });
    // The design CI tabs through these controls; every field the reader does not send is named, so a probe that reads one fails on its name.
    let state = json(cx, handle, "openWithState");
    let text = |key| state.get(key).and_then(Json::as_str).map(str::to_owned);
    let model = r#"{"opened":true,"busy":true,"committing":false,"kind":"","mime":"","name":"photo.png","always":false,"error":"","search":"","cursor":0,"rows":[],"controls":[{"name":"Field","focused":true,"enabled":true},{"name":"Applications","focused":false,"enabled":true},{"name":"Always","focused":false,"enabled":true},{"name":"Cancel","focused":false,"enabled":true},{"name":"Open","focused":false,"enabled":false}],"#;
    assert!(read(cx, handle, "openWithState", &[])
        .stdout
        .starts_with(model.split("\"controls\"").next().unwrap()));
    let controls = state.get("controls").and_then(Json::as_array).unwrap();
    assert_controls(&state, model);
    for control in controls {
        assert_eq!(control.get("visible"), Some(&Json::Bool(false)));
        assert_eq!(control.get("centre").and_then(Json::as_str), Some(""));
        assert_eq!(control.get("rect").and_then(Json::as_str), Some(""));
        assert_eq!(control.get("hovered"), Some(&Json::Bool(false)));
        assert_eq!(control.get("pressed"), Some(&Json::Bool(false)));
    }
    assert_eq!(controls[0].get("pressLive"), Some(&Json::Bool(false)));
    assert_eq!(controls[3].get("pressLive"), Some(&Json::Bool(true)));
    assert_eq!(text("unavailable").as_deref(), Some(""));
    // No frame has painted the card in this test, so every part reads empty rather than invented.
    for key in [
        "rect",
        "titleRuleRect",
        "searchRect",
        "listRect",
        "rowRect",
        "noMatchRect",
    ] {
        assert_eq!(text(key).as_deref(), Some(""), "{key}");
    }
    for key in ["rowHeight", "eyebrowHeight", "gap", "searchTrail"] {
        assert!(
            state
                .get(key)
                .and_then(Json::as_f64)
                .is_some_and(|token| token > 0.),
            "{key}"
        );
    }
    assert_eq!(
        read(cx, handle, "menuDialogState", &[]),
        read(cx, handle, "openWithState", &[]),
    );
}

#[gpui::test]
fn uish_open_with_card_reports_v0312_model_facts(cx: &mut TestAppContext) {
    let handle = open(cx);
    update(cx, handle, |shell, _, _| {
        let mut card = OpenWith::open(1, "notes.txt");
        let reply = flea::jsondoc::parse(r#"{"t":"menuaction","id":1,"op":"applications","ok":true,"path":"/fx/notes.txt","kind":"Plain text document","mime":"text/plain","applications":[{"id":"nvim.desktop","label":"Neovim","icon":"nvim","default":true}],"installed":[{"id":"xfce4-about.desktop","label":"About Xfce","icon":"","default":false}]}"#).unwrap();
        card.reply(&shell.locale.catalog, &reply);
        shell.open_with = Some(OpenWithOpen {
            side: 0,
            id: 1,
            card,
            frame: Default::default(),
            initial: Default::default(),
        });
    });
    // ui/Ipc.qml openWithState's model fields, in its names and row shapes, before the painted parts.
    let model = r#"{"opened":true,"busy":false,"committing":false,"kind":"Plain text document","mime":"text/plain","name":"notes.txt","always":false,"error":"","search":"","cursor":0,"rows":[{"eyebrow":"Registered for Plain text document","rule":false},{"id":"nvim.desktop","label":"Neovim","icon":"nvim","isDefault":true},{"eyebrow":"All applications","rule":true},{"id":"xfce4-about.desktop","label":"About Xfce","icon":"","isDefault":false}],"controls":[{"name":"Field","focused":true,"enabled":true},{"name":"Applications","focused":false,"enabled":true},{"name":"Always","focused":false,"enabled":true},{"name":"Cancel","focused":false,"enabled":true},{"name":"Open","focused":false,"enabled":true}],"#;
    let state = read(cx, handle, "openWithState", &[]).stdout;
    let state = state.trim_end();
    assert!(
        state.starts_with(model.split("\"controls\"").next().unwrap()),
        "{state}"
    );
    assert!(state.ends_with(r#","unavailable":""}"#), "{state}");
    let state = jsondoc::parse(state).unwrap();
    assert_controls(&state, model);
}
