use super::*;
use crate::backend::testdir::TestDir;

#[test]
fn fix_7zpass_menu_validation_uses_cursor_archive() {
    let root = TestDir::new("menu-extract-cursor");
    let marked = root.file("notes.txt", "marked notes");
    let archive = root.file("notes.7z", "archive under the cursor");
    let mut snapshot = Snapshot::default();
    let reply = snapshot.handle_request(
        r#"{"op":"snapshot","id":17}"#,
        vec![marked.to_str().unwrap().into()],
        archive.to_str(),
        &Registry::default(),
        &Cancellation::default(),
    );
    assert!(field_bool(&reply, "ok"), "{reply}");
    let reply = snapshot.handle(r#"{"op":"validate","id":17,"action":"extract"}"#, vec![]);
    assert!(field_bool(&reply, "ok"), "{reply}");
    assert_eq!(
        crate::json::field_str_array(&reply, "paths"),
        [archive.to_str().unwrap()]
    );
    let reply = snapshot.handle(
        r#"{"op":"validate","id":17,"action":"compress:7z"}"#,
        vec![],
    );
    assert_eq!(
        crate::json::field_str_array(&reply, "paths"),
        [marked.to_str().unwrap()]
    );
}

#[test]
fn fix_7zpass_menu_validation_refuses_a_replaced_cursor_archive() {
    let root = TestDir::new("menu-extract-replaced");
    let marked = root.file("notes.txt", "marked notes");
    let archive = root.file("notes.7z", "original archive");
    let mut snapshot = Snapshot::default();
    let reply = snapshot.handle_request(
        r#"{"op":"snapshot","id":18}"#,
        vec![marked.to_str().unwrap().into()],
        archive.to_str(),
        &Registry::default(),
        &Cancellation::default(),
    );
    assert!(field_bool(&reply, "ok"), "{reply}");
    root.replace_file(&archive, "replacement archive");
    let reply = snapshot.handle(r#"{"op":"validate","id":18,"action":"extract"}"#, vec![]);
    assert!(!field_bool(&reply, "ok"), "{reply}");
    assert!(crate::json::field_str_array(&reply, "paths").is_empty());
    assert_eq!(std::fs::read_to_string(marked).unwrap(), "marked notes");
}

#[test]
fn fix_7zpass_menu_validation_keeps_single_item_legacy_snapshots() {
    let root = TestDir::new("menu-extract-legacy");
    let archive = root.file("notes.7z", "selected archive");
    let mut snapshot = Snapshot::default();
    let reply = snapshot.handle(
        r#"{"op":"snapshot","id":19}"#,
        vec![archive.to_str().unwrap().into()],
    );
    assert!(field_bool(&reply, "ok"), "{reply}");
    let reply = snapshot.handle(r#"{"op":"validate","id":19,"action":"extract"}"#, vec![]);
    assert!(field_bool(&reply, "ok"), "{reply}");
    assert_eq!(
        crate::json::field_str_array(&reply, "paths"),
        [archive.to_str().unwrap()]
    );
}

#[test]
fn fix_7zpass_extract_dispatch_keeps_cursor_identity_and_rejects_stale_menus() {
    let root = TestDir::new("menu-extract-dispatch");
    let marked = root.file("notes.txt", "marked notes");
    let archive = root.file("notes.7z", "original archive");
    let (menu, _rx) = super::tests::unstarted_menu();
    *menu.snapshot.lock().unwrap() = Snapshot {
        id: 20,
        items: vec![Selected::inspect(marked.to_str().unwrap()).unwrap()],
        cursor: Some(Selected::inspect(archive.to_str().unwrap()).unwrap()),
        ..Default::default()
    };
    let extract = menu.extract_selection(20).unwrap();
    assert_eq!(extract[0].path, archive);
    assert_eq!(menu.selection(20).unwrap()[0].path, marked);
    assert!(menu.extract_selection(0).is_err());
    assert!(menu.extract_selection(21).is_err());
    assert!(validate_sources(Some(&extract), std::slice::from_ref(&marked)).is_err());
    root.replace_file(&archive, "replacement archive");
    assert!(validate_sources(Some(&extract), std::slice::from_ref(&archive)).is_err());
    menu.snapshot.lock().unwrap().id = 0;
    assert!(menu.extract_selection(20).is_err());
}
