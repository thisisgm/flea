use super::*;

fn failure_sentence(cx: &mut TestAppContext, message: &str) {
    failure_message(cx, message, message);
}

fn failure_message(cx: &mut TestAppContext, message: &str, expected: &str) {
    let (mut fixture, _folder) = fixture(cx);
    fixture.menu(cx, 0, vec![0]);
    update(cx, fixture.handle, |shell, window, cx| {
        shell.retire_menu(window, cx);
        shell.menu.plugin_runtime.pending.insert(71, 0);
        shell
            .plugins
            .host
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |_, cx| {
                cx.emit(crate::g2::plugins::Response {
                    id: 71,
                    plugin: "example.checksums".into(),
                    request: jsondoc::parse(r#"{"op":"run","action":"sha256"}"#).unwrap(),
                    message: jsondoc::parse(r#"{"op":"error"}"#).unwrap(),
                    ok: false,
                    error: message.into(),
                });
            });
    });
    wait(cx, |cx| {
        let failed = notice(&fixture, cx);
        failed.pending == 0 && failed.error
    });
    let failed = notice(&fixture, cx);
    assert_eq!(failed.pane, expected);
    assert_eq!(failed.shown, expected);
    assert_eq!(failed.queued, vec![expected.to_owned()]);
    assert!(failed.error);
    stop(cx, &fixture);
}

#[gpui::test]
fn plugin_footer_error_period_is_preserved(cx: &mut TestAppContext) {
    failure_sentence(cx, "Checksum does not match.");
}

#[gpui::test]
fn plugin_footer_error_question_is_preserved(cx: &mut TestAppContext) {
    failure_sentence(cx, "Did you choose the right file?");
}

#[gpui::test]
fn plugin_footer_error_exclamation_is_preserved(cx: &mut TestAppContext) {
    failure_sentence(cx, "Checksum mismatch!");
}

#[gpui::test]
fn plugin_footer_error_empty_is_reported(cx: &mut TestAppContext) {
    failure_message(cx, "", "That action could not be completed; try again.");
}

#[gpui::test]
fn plugin_footer_error_control_only_is_reported(cx: &mut TestAppContext) {
    failure_message(
        cx,
        "\u{1b}\n",
        "That action could not be completed; try again.",
    );
}
