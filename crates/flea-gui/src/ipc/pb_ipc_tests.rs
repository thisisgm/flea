//! Frozen reader contracts through the production Root's mounted adapter chain.
use super::pb_ipc_fixture::{draw, json, open, read, settle};
use flea::jsondoc::Json;
use gpui::{point, px, Modifiers, TestAppContext};

#[gpui::test]
fn pb_ipc_share_browser_cursor_rect_and_owner_follow_the_live_browser(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    assert_eq!(read(&shell, cx, "shareBrowserCursor", &[]), "0");
    assert_eq!(read(&shell, cx, "shareBrowserRect", &[]), "");
    let state = json(&shell, cx, "shareBrowserState");
    assert_eq!(state.get("active"), Some(&Json::Bool(false)));
    assert_eq!(state.get("owner").and_then(Json::as_f64), Some(-1.));
    shell.update_in(cx, |shell, window, cx| {
        let origin = shell.network_origin(0);
        let shares = shell
            .rail
            .as_ref()
            .unwrap()
            .network
            .as_ref()
            .unwrap()
            .shares
            .clone();
        shares.update(cx, |view, cx| {
            view.show(
                "smb://server".into(),
                "Server".into(),
                vec!["docs".into(), "media".into()],
                origin,
                window,
                cx,
            )
        });
        cx.notify();
    });
    draw(cx);
    let state = json(&shell, cx, "shareBrowserState");
    assert_eq!(state.get("active"), Some(&Json::Bool(true)));
    assert_eq!(state.get("owner").and_then(Json::as_f64), Some(0.));
    assert_eq!(
        state.get("baseUri").and_then(Json::as_str),
        Some("smb://server")
    );
    assert_eq!(
        state.get("rect").and_then(Json::as_str),
        Some(read(&shell, cx, "shareBrowserRect", &[]).as_str())
    );
    assert!(!read(&shell, cx, "shareBrowserRect", &[]).is_empty());
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(read(&shell, cx, "shareBrowserCursor", &[]), "1");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        json(&shell, cx, "shareBrowserState")
            .get("owner")
            .and_then(Json::as_f64),
        Some(-1.)
    );
    assert_eq!(
        read(&shell, cx, "shareBrowserCursor", &[]),
        "1",
        "a mounted closed browser retains its cursor"
    );
    assert_eq!(
        read(&shell, cx, "shareBrowserRect", &[]),
        "",
        "a closed browser has no visible rectangle"
    );
}

#[gpui::test]
fn pb_ipc_eject_hover_reads_the_pointer_on_the_actual_mark(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &["9999"]), "false");
    shell.update(cx, |shell, cx| {
        shell.network_publish(
            Some(vec![flea::ui_logic_pl1::RailEntry {
                uri: "smb://server/docs".into(),
                path: "/media/docs".into(),
                label: "Docs".into(),
                mounted: true,
                group: flea::ui_logic_pl1::Group::Network,
                kind: flea::ui_logic_pl1::Kind::Share,
                ..Default::default()
            }]),
            cx,
        );
    });
    draw(cx);
    let row = shell.read_with(cx, |shell, cx| {
        shell
            .rail
            .as_ref()
            .unwrap()
            .places
            .read(cx)
            .laid
            .borrow()
            .rows
            .iter()
            .find(|row| row.label == "Docs")
            .unwrap()
            .clone()
    });
    let hit = row
        .eject_hit()
        .expect("the mounted row draws the release mark");
    let index = row.index.to_string();
    assert_eq!(read(&shell, cx, "railMarkHovered", &[&index]), "false");
    cx.simulate_mouse_move(
        point(px(hit.x + hit.width / 2.), px(hit.y + hit.height / 2.)),
        None,
        Modifiers::none(),
    );
    draw(cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &[&index]), "true");
    cx.simulate_mouse_move(point(px(700.), px(400.)), None, Modifiers::none());
    draw(cx);
    assert_eq!(read(&shell, cx, "railMarkHovered", &[&index]), "false");
}

#[gpui::test]
fn pb_ipc_thumbnail_policy_reads_requests_answers_and_live_mode(cx: &mut TestAppContext) {
    let (shell, cx) = open(cx);
    settle(cx);
    let state = json(&shell, cx, "thumbnailPolicyState");
    assert_eq!(state.get("view").and_then(Json::as_str), Some("list"));
    assert_eq!(state.get("mode").and_then(Json::as_str), Some("media"));
    assert_eq!(state.get("previewIndex").and_then(Json::as_f64), Some(-1.));
    assert_eq!(state.get("previewPath").and_then(Json::as_str), Some(""));
    assert_eq!(state.get("previewReady"), Some(&Json::Bool(false)));
    assert_eq!(
        state.get("files").and_then(|files| files.get("0")),
        Some(&Json::Null)
    );
    assert!(state
        .get("pending")
        .unwrap()
        .as_array()
        .unwrap()
        .contains(&Json::Num("0".into())));
    shell.update_in(cx, |shell, window, cx| {
        shell.grid_pictures_event(
            0,
            &crate::backend::Backend::test_reply_line(r#"{"t":"thumbed","row":0,"file":""}"#),
            window,
            cx,
        );
    });
    let state = json(&shell, cx, "thumbnailPolicyState");
    assert_eq!(
        state
            .get("files")
            .and_then(|files| files.get("0"))
            .and_then(Json::as_str),
        Some("")
    );
    assert!(!state
        .get("pending")
        .unwrap()
        .as_array()
        .unwrap()
        .contains(&Json::Num("0".into())));
    shell.update(cx, |shell, cx| {
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"preview":{"thumbnails":"off"}}"#).unwrap();
        cx.notify();
    });
    draw(cx);
    let state = json(&shell, cx, "thumbnailPolicyState");
    assert_eq!(state.get("mode").and_then(Json::as_str), Some("off"));
    assert_eq!(state.get("files"), Some(&Json::Obj(vec![])));
}
