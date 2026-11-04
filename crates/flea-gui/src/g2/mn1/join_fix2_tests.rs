//! Independent regression probes for the round 2 observer fixes.
use super::join_tests::{open, read, receive};
use flea::jsondoc::{self, Json};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn rev_u06_r2_openwith_leaf_icons_survive_menu_state(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    let handle = fixture.handle;
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("m");
    let request = (fixture.requests)().expect("snapshot request");
    let id = flea::json::field_usize(&request, "id").unwrap();
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"snapshot\",\"ok\":true,\"kinds\":[32768]}}"));
    assert!((fixture.requests)()
        .unwrap()
        .contains("\"op\":\"applications\""));
    receive(cx, handle, &format!("{{\"t\":\"menuaction\",\"id\":{id},\"op\":\"applications\",\"ok\":true,\"applications\":[{{\"id\":\"zzflea-openwith.desktop\",\"label\":\"Zzflea Fixture\",\"icon\":\"text-x-generic\",\"default\":true}},{{\"id\":\"other.desktop\",\"label\":\"Other\",\"icon\":\"/fixture/icons/other.svg\",\"default\":false}}]}}"));
    let raw = read(cx, handle, "menuState").stdout;
    println!("R2_OPENWITH_STATE {}", raw.trim());
    let state = jsondoc::parse(raw.trim()).unwrap();
    let leaves = state
        .get("entries")
        .and_then(Json::as_array)
        .unwrap()
        .iter()
        .find(|row| row.get("action").and_then(Json::as_str) == Some("openWith"))
        .unwrap()
        .get("submenu")
        .and_then(Json::as_array)
        .unwrap();
    assert_eq!(
        leaves[0].get("icon").and_then(Json::as_str),
        Some("text-x-generic"),
        "tests/ui-openwith-design.sh:95 reads the producer's icon name from menuState leaves"
    );
    assert!(
        leaves.iter().any(|row| row
            .get("icon")
            .and_then(Json::as_str)
            .is_some_and(|icon| icon.starts_with('/'))),
        "tests/ui-openwith-design.sh:93 reads resolved icon paths from menuState leaves"
    );
    let model = jsondoc::parse(read(cx, handle, "contextMenuModel").stdout.trim()).unwrap();
    let parent = model
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row.get("action").and_then(Json::as_str) == Some("openWith"))
        .unwrap();
    assert_eq!(parent.get("submenu").and_then(Json::as_array), Some(leaves));
    assert!(
        parent.get("icon").is_none(),
        "no icon is invented for the parent row"
    );
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes("j right");
    let opened = jsondoc::parse(read(cx, handle, "menuState").stdout.trim()).unwrap();
    let flyout = opened
        .get("submenuEntries")
        .and_then(Json::as_array)
        .unwrap();
    assert_eq!(
        flyout[0].get("icon").and_then(Json::as_str),
        Some("text-x-generic")
    );
    // The default stands alone: a separator, then the other apps, then Another Application.
    assert!(flyout[1].get("separator").and_then(Json::as_bool) == Some(true));
    assert_eq!(
        flyout[2].get("icon").and_then(Json::as_str),
        Some("/fixture/icons/other.svg")
    );
    assert!(flyout.last().unwrap().get("icon").is_none());
}
