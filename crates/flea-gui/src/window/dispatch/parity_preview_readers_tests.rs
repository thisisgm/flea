//! Reader contracts use the mounted Root and the same live models the preview draws.
use super::{preview_tests::{answer, body, close, code_reply, draw, opened, press, rows, sent, token}, test_shell::{land, set_cursor}};
use crate::{g2::pv1::{Body, doc::View}, window::u07_tests::{read, press as click}};
use flea::jsondoc::{self, Json};
use gpui::TestAppContext;

#[gpui::test]
fn pb_preview_text_length_and_tail_use_exact_utf16_source(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 1);
    press(cx, "space");
    let ask = sent(&mut requests, "code");
    answer(&shell, cx, code_reply(token(&ask[0]), &["fixture"], 1));
    let Some(Body::Doc(doc)) = body(&shell, cx) else { panic!("code mounted") };
    let View::Code(code) = doc.view else { panic!("code mounted") };
    let text = format!("{}\u{1d11e}\n", "x".repeat(5000));
    code.update(cx, |code, cx| code.set_subject("/files/notes.txt", 5005, true, Some(text.clone()), cx));
    draw(cx);
    assert_eq!(read(&shell, cx, "previewTextLength", &[]), "5003");
    assert_eq!(read(&shell, cx, "previewTextTail", &["-4"]), "");
    assert_eq!(read(&shell, cx, "previewTextTail", &["0"]), "");
    assert_eq!(read(&shell, cx, "previewTextTail", &["3"]), "\u{1d11e}");
    assert_eq!(read(&shell, cx, "previewTextTail", &["90000"]).encode_utf16().count(), 4095);
    press(cx, "escape");
    assert_eq!(read(&shell, cx, "previewTextLength", &[]), "0");
    close(&shell, cx);
}

#[gpui::test]
fn pb_preview_media_readers_report_drawn_controls_and_mute_input(cx: &mut TestAppContext) {
    let (shell, cx, _requests, _script) = super::quick_look_body_tests::mounted(cx);
    super::quick_look_body_tests::open_kind(&shell, cx, "song.ogg");
    let state = jsondoc::parse(&read(&shell, cx, "previewStrip", &[])).unwrap();
    assert_eq!(state.get("visible").and_then(Json::as_bool), Some(true));
    assert_eq!(state.get("muted").and_then(Json::as_bool), Some(false));
    let centre = state.get("mute").and_then(Json::as_str).unwrap();
    assert_eq!(centre.split_whitespace().count(), 2);
    assert_eq!(read(&shell, cx, "previewSliderCentre", &[]).split_whitespace().count(), 2);
    click(cx, centre, 1);
    draw(cx);
    let state = jsondoc::parse(&read(&shell, cx, "previewStrip", &[])).unwrap();
    assert_eq!(state.get("muted").and_then(Json::as_bool), Some(true));
    press(cx, "escape");
    assert_eq!(read(&shell, cx, "previewSliderCentre", &[]), "");
    close(&shell, cx);
}

#[gpui::test]
fn pb_preview_picture_rect_names_the_quick_look_picture_not_the_column(cx: &mut TestAppContext) {
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 2);
    press(cx, "space");
    let ask = sent(&mut requests, "meta");
    answer(&shell, cx, format!(r#"{{"t":"meta","row":2,"token":{},"w":800,"h":600,"orient":1}}"#, token(&ask[0])));
    let expected = cx.debug_bounds("pv1-picture").expect("the actual picture container");
    let value = read(&shell, cx, "previewPictureRect", &[]);
    assert_eq!(value, format!("{} {} {} {}", f32::from(expected.left()).round(), f32::from(expected.top()).round(), f32::from(expected.size.width).round(), f32::from(expected.size.height).round()));
    press(cx, "escape");
    assert_eq!(read(&shell, cx, "previewPictureRect", &[]), "");
    close(&shell, cx);
}

#[gpui::test]
fn pb_preview_external_thumbnail_menu_toggles_the_current_class(cx: &mut TestAppContext) {
    use gpui::{point, px, Modifiers, MouseButton, MouseDownEvent};
    let (shell, cx) = super::test_shell::open_root_with(cx, r#""keys":"default","view":"list","menu":{"hidden":[]}"#, None);
    land(&shell, cx, "/files", rows(), 1);
    answer(&shell, cx, r#"{"t":"fsinfo","path":"/files","fs":"cifs","free":90,"class":"network"}"#.into());
    cx.simulate_event(MouseDownEvent { position: point(px(950.), px(500.)), button: MouseButton::Right, click_count: 1, modifiers: Modifiers::none(), first_mouse: false });
    super::menus_tests::settle(cx);
    let entries = read(&shell, cx, "contextMenuEntries", &[]);
    assert!(entries.contains("Show Network Thumbnails"), "the background offers the known class: {entries}");
    super::menus_tests::choose(&shell, cx, "extThumbs");
    assert_eq!(shell.read_with(cx, |shell, _| shell.launch.settings.data.get("preview").unwrap().get("thumbNetwork").and_then(Json::as_bool)), Some(true));
    assert_eq!(super::test_shell::status(&shell, cx), "Network thumbnails are on");
    close(&shell, cx);
}

#[gpui::test]
fn pb_preview_open_with_button_opens_the_listed_subjects_applications(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton};
    let (shell, cx, mut requests) = opened(cx);
    land(&shell, cx, "/files", rows(), 1);
    set_cursor(&shell, cx, 3);
    press(cx, "space");
    let button = cx.debug_bounds("pv1-open-with").expect("Open With button");
    cx.simulate_mouse_down(button.center(), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(button.center(), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    let snapshot = sent(&mut requests, "menuaction");
    assert!(snapshot.iter().any(|ask| ask.get("op").and_then(Json::as_str) == Some("snapshot") && ask.get("rows").and_then(Json::as_array).is_some_and(|rows| rows.len() == 1 && rows[0].as_f64() == Some(3.))), "Open With snapshots the card's row: {snapshot:?}");
    super::menus_tests::answer(&shell, cx, &[0o100644]);
    assert!(read(&shell, cx, "contextMenuVisible", &[]).parse::<bool>().unwrap());
    close(&shell, cx);
}
