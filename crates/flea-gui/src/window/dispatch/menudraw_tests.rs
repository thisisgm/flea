//! A rail menu opened by key and a blank-ground right click in an empty folder are both drawn in a real frame.
use super::{
    menus_tests::{right_click, settle},
    test_shell::{land, open_root_with, row},
};
use crate::{g2::context_menus::element::PAINTED, state::Shell, window::u07_tests::read};
use flea::{
    jsondoc::{self, Json},
    ui_logic_rail_places::{
        order::{Heading, RailSection},
        rail::{Group, Kind, RailEntry},
    },
};
use gpui::{Bounds, Entity, Pixels, RequestFrameOptions, TestAppContext, VisualTestContext};

fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    jsondoc::parse(&read(shell, cx, "menuState", &[])).unwrap()
}

fn painted() -> Vec<Bounds<Pixels>> {
    PAINTED.with(|painted| painted.borrow().clone())
}

// Only the native frame callback draws, and it draws only a window something marked dirty.
fn native_frames(cx: &mut VisualTestContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.simulate_frame_request(RequestFrameOptions::default());
    }
    cx.run_until_parked();
}

#[gpui::test]
fn menudraw_rail_key_menu_is_painted_under_its_favourite(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let (shell, cx) = open_root_with(cx, r#""view":"list","keys":"default""#, None);
    let record = jsondoc::parse(r#"{"path":"/docs","label":"Docs"}"#).unwrap();
    shell.update(cx, |shell, _| {
        let patch =
            jsondoc::parse(r#"{"places":{"favourites":[{"path":"/docs","label":"Docs"}]}}"#)
                .unwrap();
        shell.launch.settings.data =
            flea::uistate::patched(&shell.launch.settings.data, &patch).unwrap();
    });
    land(&shell, cx, "/files", vec![row("notes.txt", false)], 1);
    let producer = shell.read_with(cx, |shell, _| shell.rail.as_ref().unwrap().places.clone());
    producer.update(cx, |producer, cx| {
        producer.replace_sections(
            vec![RailSection {
                heading: Heading::Key("places.heading.favourites"),
                rows: vec![RailEntry {
                    group: Group::Favourite,
                    kind: Kind::Favourite,
                    label: "Docs".into(),
                    path: "/docs".into(),
                    glyph: "folder",
                    favourite_index: Some(0),
                    original: Some(record),
                    ..RailEntry::default()
                }],
            }],
            cx,
        )
    });
    settle(cx);
    cx.simulate_keystrokes("tab down");
    settle(cx);
    assert_eq!(
        read(&shell, cx, "railCursor", &[]).trim(),
        "0",
        "the rail cursor walks to Docs"
    );
    let at = read(&shell, cx, "railRowCentre", &["0"]);
    let xy: Vec<f32> = at.split_whitespace().map(|v| v.parse().unwrap()).collect();
    assert_eq!(xy.len(), 2, "the Docs row is drawn: {at:?}");
    PAINTED.with(|painted| painted.borrow_mut().clear());
    cx.simulate_keystrokes("m");
    native_frames(cx);
    assert_eq!(
        state(&shell, cx).get("opened").and_then(Json::as_bool),
        Some(true)
    );
    let frames = painted();
    let frame = frames
        .last()
        .expect("the rail menu is painted in a native frame");
    let (top, left) = (f32::from(frame.origin.y), f32::from(frame.origin.x));
    assert!(
        top > xy[1] && top < xy[1] + 40.,
        "the menu opens under Docs: top {top}, row centre {}",
        xy[1]
    );
    assert!(
        left < xy[0],
        "the menu starts at the row's lead: left {left}, centre {}",
        xy[0]
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}

#[gpui::test]
fn menudraw_blank_ground_right_click_opens_the_background_menu_in_an_empty_folder(
    cx: &mut TestAppContext,
) {
    let (shell, cx) = open_root_with(cx, r#""view":"list","keys":"default""#, None);
    land(&shell, cx, "/files", Vec::new(), 1);
    let kind = shell.read_with(cx, |shell, _| {
        shell.panes[0].as_ref().unwrap().pane.state.kind
    });
    assert_eq!(kind, crate::pane::states::Kind::Empty, "the hero shows");
    let pane = cx.update(|window, cx| {
        shell.update(cx, |shell, _| {
            shell
                .geometry(window)
                .rows(0, false, shell.slots.strip_height)
        })
    });
    let (x, y) = (pane.x + pane.width * 0.65, pane.y + pane.height * 0.85);
    PAINTED.with(|painted| painted.borrow_mut().clear());
    right_click(cx, x, y);
    let menu = state(&shell, cx);
    assert_eq!(
        menu.get("opened").and_then(Json::as_bool),
        Some(true),
        "a right click on blank ground opens the background menu"
    );
    assert_eq!(menu.get("hasRow").and_then(Json::as_bool), Some(false));
    let frames = painted();
    let frame = frames.last().expect("the background menu is painted");
    let (left, top) = (f32::from(frame.origin.x), f32::from(frame.origin.y));
    let (w, h) = (f32::from(frame.size.width), f32::from(frame.size.height));
    assert!(
        (left - x).abs() < 2. || (left + w - x).abs() < 2.,
        "the menu opens at the pointer's x: {left}..{} vs {x}",
        left + w
    );
    assert!(
        (top - y).abs() < 2. || (top + h - y).abs() < 2. || (top <= y && y <= top + h),
        "the menu opens at the pointer's y: {top}..{} vs {y}",
        top + h
    );
    shell.update(cx, |shell, _| shell.stop_for_app_quit());
}
