//! Native dialog regressions retain the refused draft and its actual error detail.
use super::{
    shell::Ink,
    tests::{english, notes, open_with, permissions},
    Effect,
};
use flea::jsondoc::{parse, Json};

#[test]
fn fxuish_def2_permissions_reader_waits_for_painted_focus() {
    let mut model = permissions();
    let painted = model.card(&english());
    model.focus = super::permissions::Stop::Box(0);
    let probe = Default::default();
    let read = |painted: Option<&super::shell::Card>| {
        let state = model.painted_state(&english(), painted, &probe);
        state
            .get("controls")
            .and_then(Json::as_array)
            .unwrap()
            .iter()
            .filter(|control| control.get("focused").and_then(Json::as_bool) == Some(true))
            .map(|control| {
                control
                    .get("name")
                    .and_then(Json::as_str)
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(read(Some(&painted)), ["Apply"]);
    assert_eq!(read(Some(&model.card(&english()))), ["Owner read"]);
    assert!(
        read(None).is_empty(),
        "no painted frame has no painted focus"
    );
}

#[test]
fn fxuish_def2_permissions_multi_refusal_uses_error_ink() {
    let paths = vec!["/test/one.txt".into(), "/test/special.txt".into()];
    let (mut model, _) = super::permissions::Permissions::open(9, paths, "");
    model.reply(
        &english(),
        &parse(r#"{"op":"inspect","id":9000,"ok":true,"mode":"0644","reason":""}"#).unwrap(),
    );
    model.reply(&english(), &parse(r#"{"op":"inspect","id":9001,"ok":true,"mode":"4644","reason":"Read-only: setuid bit is present."}"#).unwrap());
    assert!(notes(&model.card(&english()))
        .iter()
        .any(|(text, ink)| text.contains("setuid") && *ink == Ink::Error));
}

#[test]
fn fxuish_def2_permissions_refusal_uses_error_ink() {
    let mut model = permissions();
    model.reply(&english(), &parse(r#"{"op":"inspect","id":7,"ok":true,"mode":"4755","reason":"Setuid modes cannot be changed here."}"#).unwrap());
    assert!(notes(&model.card(&english()))
        .contains(&("Setuid modes cannot be changed here.".into(), Ink::Error,)));
}

#[test]
fn fxuish_def2_stopped_permissions_keep_the_form_and_disabled_apply() {
    let mut model = permissions();
    model.text = "0600".into();
    model.stopped();
    let card = model.card(&english());
    assert_eq!(card.buttons.len(), 2, "the reviewed form retains Apply");
    assert!(!card.buttons[1].enabled);
    assert_eq!(model.text, "0600");
    assert!(notes(&card)
        .iter()
        .any(|(text, ink)| text.contains("Restart Flea") && *ink == Ink::Error));
}

#[test]
fn fxuish_def2_openwith_shows_launch_detail_and_falls_back_only_when_empty() {
    for detail in [Some("The application exited with code 23."), Some(""), None] {
        let mut model = open_with();
        assert!(model
            .press_at(1)
            .iter()
            .any(|effect| matches!(effect, Effect::Send(_))));
        let field = detail.map_or(String::new(), |text| format!(",\"error\":\"{text}\""));
        model.reply(
            &english(),
            &parse(&format!(r#"{{"id":4,"op":"openWith","ok":false{field}}}"#)).unwrap(),
        );
        let expected = detail
            .filter(|text| !text.is_empty())
            .unwrap_or("The application could not be opened.");
        assert_eq!(model.error(), Some(expected));
        assert!(model.can_open(), "a refusal permits retry");
    }
}
