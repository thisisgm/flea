//! A provider cannot resize a painted menu, and the existing loading mark cannot blink past its minimum stand.
use super::join_tests::{open, read, receive};
use gpui::{AppContext, TestAppContext, VisualTestContext};
use std::time::Duration;

#[gpui::test]
fn uish_misc_phone_folder_menu_uses_the_same_trash_guard_as_the_key(cx: &mut TestAppContext) {
    let fixture = open(cx);
    fixture
        .handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, _| {
                shell.panes[0].as_mut().unwrap().path =
                    "/run/user/1000/gvfs/mtp:host=Phone/DCIM".into();
                let context = shell.listing_menu_context(0, &[0], 0, false);
                assert!(!context.can_trash);
                assert!(!flea::ui_logic_mn1::build::listing_entries(&context)
                    .iter()
                    .any(|entry| entry.action == "trash"));
                shell.panes[0].as_mut().unwrap().path = "/fixture/gvfs/mtp:host=Phone/DCIM".into();
                assert!(
                    shell.listing_menu_context(0, &[0], 0, false).can_trash,
                    "an ordinary folder named gvfs remains trashable"
                );
            })
        })
        .unwrap();
}

#[gpui::test]
fn u06_live_menu_discovery_publishes_one_complete_inventory(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, _| {
                shell.launch.settings.data =
                    flea::jsondoc::parse("{\"menu\":{\"hidden\":[]}}").unwrap();
            })
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    let snapshot = (fixture.requests)().unwrap();
    let id = flea::json::field_usize(&snapshot, "id").unwrap();
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768]}}"));
    assert!((fixture.requests)().unwrap().contains("__menuScripts"));
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"validate\",\"action\":\"__menuScripts\",\"ok\":true,\"paths\":[\"/files/a\"]}}"));
    assert!((fixture.requests)().unwrap().contains("\"c\":\"actions\""));
    receive(cx, handle, &format!("{{\"t\":\"actions\",\"id\":{id},\"actions\":[{{\"id\":\"script:probe\",\"name\":\"Probe\",\"script\":true}}]}}"));
    assert!((fixture.requests)()
        .unwrap()
        .contains("\"op\":\"applications\""));
    assert_eq!(read(cx, handle, "contextMenuModel").stdout.trim(), "[]");
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[]}}"));
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "true\n");
    let model = read(cx, handle, "contextMenuModel").stdout;
    assert!(
        model.contains("runScript"),
        "the first painted inventory includes scripts"
    );
    assert!(
        !model.contains("openWith"),
        "an empty application producer was resolved before paint"
    );
}

#[gpui::test]
fn u06_live_menu_slow_preparation_holds_loading_for_its_minimum_stand(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(149));
    cx.run_until_parked();
    handle
        .update(cx, |root, _, cx| {
            assert!(!root.shell.read(cx).menu.presentation.loading)
        })
        .unwrap();
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    handle
        .update(cx, |root, _, cx| {
            assert!(root.shell.read(cx).menu.presentation.loading)
        })
        .unwrap();
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        assert!(window.simulate_next_frame(cx) > 0);
    })
    .unwrap();
    cx.run_until_parked();
    super::join_tests::ready(cx, &mut fixture);
    cx.run_until_parked();
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    cx.executor().advance_clock(Duration::from_millis(99));
    cx.run_until_parked();
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "true\n");
    handle
        .update(cx, |root, _, cx| {
            assert!(!root.shell.read(cx).menu.presentation.loading)
        })
        .unwrap();
}

#[gpui::test]
fn u06_live_menu_minimum_inventory_keeps_unhideable_actions_usable(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, _| {
                let hidden = flea::ui_logic_mn1::inventory::INVENTORY
                    .iter()
                    .map(|entry| flea::jsondoc::Json::Str(entry.id.into()))
                    .collect();
                shell.launch.settings.data = flea::jsondoc::Json::Obj(vec![(
                    "menu".into(),
                    flea::jsondoc::Json::Obj(vec![(
                        "hidden".into(),
                        flea::jsondoc::Json::Arr(hidden),
                    )]),
                )]);
            })
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    super::join_tests::ready(cx, &mut fixture);
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "true\n");
    let model = flea::jsondoc::parse(read(cx, handle, "contextMenuModel").stdout.trim()).unwrap();
    let actions: Vec<_> = model
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|row| row.get("action").and_then(flea::jsondoc::Json::as_str))
        .collect();
    assert_eq!(
        actions,
        ["open"],
        "Show Hidden Files now leaves the file menu by its switch; Open alone cannot"
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("escape");
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    assert!((fixture.requests)().unwrap().contains("\"op\":\"close\""));
    handle
        .update(cx, |root, _, cx| {
            let shell = root.shell.read(cx);
            assert!(!shell.panes[0].as_ref().unwrap().nav.gate.owned());
            assert!(shell.menu.captured.is_none());
            assert_eq!(shell.keyboard.context, flea::keys::Context::Listing);
        })
        .unwrap();
}

#[gpui::test]
fn uish_misc_a_catalogue_that_never_answers_does_not_keep_the_menu_shut(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, _| {
                shell.launch.settings.data =
                    flea::jsondoc::parse("{\"menu\":{\"hidden\":[\"runScript\"]}}").unwrap();
            })
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    let snapshot = (fixture.requests)().unwrap();
    let id = flea::json::field_usize(&snapshot, "id").unwrap();
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768]}}"));
    assert!((fixture.requests)()
        .unwrap()
        .contains("\"op\":\"applications\""));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        super::discovery::APPLICATIONS_WAIT_MS - 1,
    ));
    cx.run_until_parked();
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "false\n");
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        assert!(window.simulate_next_frame(cx) > 0);
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "true\n");
    assert!(read(cx, handle, "contextMenuModel")
        .stdout
        .contains("rename"));
}

// A catalogue past the deadline has not said the file has no applications, so Open With stays until its answer lands.
#[gpui::test]
fn f07_a_late_catalogue_keeps_open_with_until_its_answer(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, _| {
                shell.launch.settings.data =
                    flea::jsondoc::parse("{\"menu\":{\"hidden\":[\"runScript\"]}}").unwrap();
            })
        })
        .unwrap();
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    let snapshot = (fixture.requests)().unwrap();
    let id = flea::json::field_usize(&snapshot, "id").unwrap();
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768]}}"));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        super::discovery::APPLICATIONS_WAIT_MS,
    ));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        assert!(window.simulate_next_frame(cx) > 0);
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(read(cx, handle, "contextMenuVisible").stdout, "true\n");
    let model = read(cx, handle, "contextMenuModel").stdout;
    assert!(
        model.contains("openWith"),
        "the deadline keeps the row: {model}"
    );
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[{{\"id\":\"org.example.Edit.desktop\",\"label\":\"Example Edit\",\"icon\":\"\",\"default\":true}}]}}"));
    cx.run_until_parked();
    assert!(!model.contains("Example Edit"), "{model}");
    let filled = read(cx, handle, "contextMenuModel").stdout;
    assert!(
        filled.contains("Example Edit"),
        "the late answer fills it: {filled}"
    );
}

// An open card keeps its rows in place, so a late empty answer leaves Open With on Another Application.
#[gpui::test]
fn f07_a_late_empty_catalogue_keeps_the_open_rows_in_place(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    let snapshot = (fixture.requests)().unwrap();
    let id = flea::json::field_usize(&snapshot, "id").unwrap();
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768]}}"));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(
        super::discovery::APPLICATIONS_WAIT_MS,
    ));
    cx.run_until_parked();
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        assert!(window.simulate_next_frame(cx) > 0);
    })
    .unwrap();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    let before = read(cx, handle, "contextMenuModel").stdout;
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[]}}"));
    cx.run_until_parked();
    assert!(before.contains("openWith"), "{before}");
    assert_eq!(read(cx, handle, "contextMenuModel").stdout, before);
}

#[gpui::test]
fn uish_misc_script_jobs_keep_their_backend_side(cx: &mut gpui::TestAppContext) {
    let fixture = open(cx);
    let handle = fixture.handle;
    handle
        .update(cx, |root, _, cx| {
            root.shell.update(cx, |shell, _| {
                shell.view = crate::window::startup::View::Dual;
                shell.panes[1] = Some(crate::window::pane::PaneHost::new(
                    "/right".into(),
                    None,
                    620.,
                    &shell.locale,
                ));
                let _ = shell
                    .menu
                    .actions
                    .run("script:left.sh", vec!["/files/a".into()]);
            })
        })
        .unwrap();
    let deliver = |cx: &mut gpui::TestAppContext, side, line: &str| {
        handle
            .update(cx, |root, _, cx| {
                root.shell.update(cx, |shell, cx| {
                    shell.actions_reply(side, line, cx);
                })
            })
            .unwrap();
    };
    deliver(
        cx,
        0,
        r#"{"t":"actionstarted","requestId":1,"id":40,"ok":true,"error":""}"#,
    );
    deliver(
        cx,
        1,
        r#"{"t":"job","job":{"id":40,"state":"failed","output":["right failure"]}}"#,
    );
    assert_eq!(
        read(cx, handle, "lastMessage").stdout.trim(),
        "",
        "right backend's colliding job id must not consume the left script"
    );
    deliver(
        cx,
        0,
        r#"{"t":"job","job":{"id":40,"state":"failed","output":["left failure"]}}"#,
    );
    assert_eq!(
        read(cx, handle, "lastMessage").stdout.trim(),
        "left.sh \u{b7} left failure"
    );
}
