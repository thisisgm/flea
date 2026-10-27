//! Preview parity through the mounted Root, real keys and live IPC readers.
use super::{
    preview_tests::{answer, close, code_reply, draw, opened, press, rows, sent, token},
    test_shell::{land, open_root_with, set_cursor},
};
use crate::{g2::pv1::{Body, doc::View}, window::u07_tests::read};
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

#[gpui::test]
fn pb_preview_text_state_tracks_the_live_code_and_wrap_key(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let ask = sent(&mut requests, "code");
    answer(&shell, cx, code_reply(token(&ask[0]), &["first", "second"], 2));
    let value = jsondoc::parse(&read(&shell, cx, "previewTextState", &["false"])).unwrap();
    assert_eq!(value.get("path").and_then(Json::as_str), Some("/files/notes.txt"));
    assert_eq!(value.get("ready").and_then(Json::as_bool), Some(true));
    assert_eq!(value.get("wrap").and_then(Json::as_bool), Some(false));
    assert_eq!(value.get("total").and_then(Json::as_f64), Some(2.));
    press(cx, "w");
    let value = jsondoc::parse(&read(&shell, cx, "previewTextState", &["false"])).unwrap();
    assert_eq!(value.get("wrap").and_then(Json::as_bool), Some(true));
    press(cx, "escape");
    assert_eq!(read(&shell, cx, "previewTextState", &["false"]), "{}");
    close(&shell, cx);
}

#[gpui::test]
fn pb_preview_wrap_restores_and_keeps_the_key_across_reopen(cx: &mut TestAppContext) {
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"list","preview":{"wrap":true}"#, None);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let wrap = |shell: &crate::state::Shell, cx: &gpui::App| {
        let look = shell.quick_look.as_ref().unwrap().view.read(cx);
        let Body::Doc(doc) = &look.subject().unwrap().body else { panic!("code mounted") };
        let View::Code(code) = &doc.view else { panic!("code mounted") };
        code.read(cx).wrap
    };
    assert!(shell.read_with(cx, wrap), "stored wrap reaches the mounted code");
    press(cx, "w");
    assert!(!shell.read_with(cx, wrap));
    assert_eq!(shell.read_with(cx, |shell, _| shell.launch.settings.data.get("preview").unwrap().get("wrap").and_then(Json::as_bool)), Some(false));
    press(cx, "escape");
    press(cx, "space");
    assert!(!shell.read_with(cx, wrap), "reopen uses the kept wrap");
    close(&shell, cx);
}

#[gpui::test]
fn pb_preview_volume_restores_on_first_media_and_keeps_a_real_wheel(cx: &mut TestAppContext) {
    use crate::g2::pv3::sound::MediaSound;
    use gpui::{point, px, Modifiers, ScrollDelta, ScrollWheelEvent, TouchPhase};
    cx.executor().allow_parking();
    let (shell, cx) = open_root_with(cx, r#""keys":"default","view":"list","preview":{"volume":37}"#, None);
    land(&shell, cx, "/files", vec![super::preview_tests::file("song.ogg", "audio-ogg")], 1);
    press(cx, "space");
    assert_eq!(cx.read(MediaSound::get).volume(), 37);
    let centre = read(&shell, cx, "previewSliderCentre", &[]);
    let coords: Vec<f32> = centre.split_whitespace().map(|n| n.parse().unwrap()).collect();
    assert_eq!(coords.len(), 2, "the mounted seek track has a centre");
    cx.simulate_event(ScrollWheelEvent { position: point(px(coords[0]), px(coords[1])), delta: ScrollDelta::Lines(point(0., 1.)), modifiers: Modifiers::none(), touch_phase: TouchPhase::Moved });
    draw(cx);
    assert_eq!(cx.read(MediaSound::get).volume(), 42);
    assert_eq!(shell.read_with(cx, |shell, _| shell.launch.settings.data.get("preview").unwrap().get("volume").and_then(Json::as_f64)), Some(42.));
    close(&shell, cx);
}
