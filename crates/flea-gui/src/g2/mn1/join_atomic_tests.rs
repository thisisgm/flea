//! A provider cannot resize a painted menu, and the existing loading mark cannot blink past its minimum stand.
use super::join_tests::{open, read, receive};
use gpui::{AppContext, TestAppContext, VisualTestContext};
use std::time::Duration;

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
    assert_eq!(actions, ["open", "toggleHidden"]);
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
