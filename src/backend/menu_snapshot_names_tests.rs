use super::menu_actions::Snapshot;
use crate::backend::testdir::TestDir;
use flea::jsondoc::{self, Json};

#[test]
fn plugin_runtime_snapshot_names_are_selected_basenames_in_kind_order() {
    let dir = TestDir::new("plugin-snapshot-names");
    let one = dir.file("notes.txt", "one");
    let two = dir.file("quoted\"name.png", "two");
    dir.file("unselected-secret", "not selected");
    let reply = Snapshot::default().handle(
        r#"{"op":"snapshot","id":1}"#,
        vec![two.display().to_string(), one.display().to_string()],
    );
    let reply = jsondoc::parse(&reply).unwrap();
    let names = reply.get("names").and_then(Json::as_array).unwrap();
    assert_eq!(
        names,
        &[
            Json::Str("quoted\"name.png".into()),
            Json::Str("notes.txt".into())
        ]
    );
    assert_eq!(
        reply.get("kinds").and_then(Json::as_array).unwrap().len(),
        names.len()
    );
    assert!(!flea::ui_logic_plugin_settings::wire::compact(&reply).contains("unselected-secret"));
    assert!(names
        .iter()
        .all(|name| !name.as_str().unwrap().contains('/')));
}

#[test]
fn plugin_runtime_snapshot_omits_optional_names_above_512_items() {
    let dir = TestDir::new("plugin-snapshot-name-bound");
    let one = dir.file("notes.txt", "one");
    let reply = Snapshot::default().handle(
        r#"{"op":"snapshot","id":1}"#,
        vec![one.display().to_string(); 513],
    );
    let reply = jsondoc::parse(&reply).unwrap();
    assert!(reply.get("names").is_none());
    assert_eq!(
        reply.get("kinds").and_then(Json::as_array).unwrap().len(),
        513
    );
}

#[path = "fixlogic2_menu_tests.rs"]
mod fixlogic2;
