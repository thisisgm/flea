use super::super::{
    preview_tests::{close, draw, file},
    previewtypes_fixture_tests::{answer, ask, field, local, meta, pump},
    test_shell::land,
};
use crate::window::u07_tests::read;
use flea::jsondoc::Json;
use gpui::TestAppContext;

#[gpui::test]
fn a_bounded_3mf_keeps_the_existing_failed_model_column_recipe(cx: &mut TestAppContext) {
    let (shell, cx) = super::super::previewtypes_tests::open_at(cx, "/files", Some(14.));
    land(
        &shell,
        cx,
        "/files",
        vec![file("large.3mf", "application-octet-stream")],
        1,
    );
    let mut lines = local(&shell, cx, "/files");
    meta(&shell, cx, &lines, "");
    lines.extend(pump(cx));
    let model = ask(&lines, "model3d");
    let request = flea::jsondoc::parse(model).unwrap();
    let slot = request.get("slot").and_then(Json::as_str).unwrap();
    let token = field(model, "token");
    answer(
        &shell,
        cx,
        &format!(
            r#"{{"t":"model3d","slot":"{slot}","token":{token},"state":"failed","reason":"new diagnostic wording","refusal":"too_large"}}"#,
        ),
    );
    pump(cx);
    draw(cx);
    shell.read_with(cx, |shell, cx| {
        let view = shell.panes[0]
            .as_ref()
            .unwrap()
            .preview
            .as_ref()
            .unwrap()
            .read(cx);
        let frame = view.drawn.as_ref().expect("refusal frame mounted");
        let crate::window::selection_preview::Mounted::Model(model) = &frame.mounted else {
            panic!("model view mounted");
        };
        assert_eq!(
            model.read(cx).failed().as_deref(),
            Some("This file is too large to preview.")
        );
        assert!(view.failure(frame, cx).is_none());
        assert_eq!(view.state(frame, cx), frame.kind.state());
        assert_ne!(
            view.state(frame, cx),
            flea::ui_logic_preview_core::kinds::State::Error
        );
        assert_eq!(frame.subject.row.name, "large.3mf");
    });
    assert_eq!(read(&shell, cx, "total", &[]), "1");
    close(&shell, cx);
}
