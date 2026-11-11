//! Removing the mounted Open With card retires its pointer receipt, including retained frame handles.
use super::*;
use crate::window::u07_tests::read;
use flea::jsondoc::{self, Json};
use gpui::{point, px, Modifiers};

fn draw(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(600));
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

#[gpui::test]
fn cohesion_open_with_unmount_retires_pointer_flags(cx: &mut TestAppContext) {
    let (shell, cx, id) = opened(cx);
    answer(&shell, cx, id);
    draw(cx);
    let frame = shell.read_with(cx, |shell, _| {
        shell.open_with.as_ref().unwrap().frame.clone()
    });
    let centre = frame
        .part("field")
        .expect("the actual search field painted")
        .bounds
        .center();
    cx.simulate_mouse_move(centre, None, Modifiers::none());
    draw(cx);
    assert!(
        frame.pointer("field").0,
        "the real hover handler recorded entry"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(shell.read_with(cx, |shell, _| shell.open_with.is_none()));
    assert_eq!(
        frame.pointer("field"),
        (false, false),
        "an unmounted card cannot retain hover/press"
    );
    assert!(frame.part("field").is_none(), "its geometry is retired too");
    cx.simulate_mouse_move(point(px(5.), px(5.)), None, Modifiers::none());
    cx.update(|_, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_open_with(0, "/files/mystery.xyz".into(), cx)
        })
    });
    let id = shell.read_with(cx, |shell, _| shell.open_with.as_ref().unwrap().id);
    deliver(
        &shell,
        cx,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[33188]}}"#),
    );
    answer(&shell, cx, id);
    draw(cx);
    let state = jsondoc::parse(&read(&shell, cx, "openWithState", &[])).unwrap();
    let controls = state.get("controls").and_then(Json::as_array).unwrap();
    let field = controls
        .iter()
        .find(|row| row.get("name").and_then(Json::as_str) == Some("Field"))
        .unwrap();
    assert_eq!(field.get("hovered").and_then(Json::as_bool), Some(false));
    quit(&shell, cx);
}
