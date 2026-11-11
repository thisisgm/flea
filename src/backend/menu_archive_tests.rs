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
    let extract = menu.extract_selection(20, &archive).unwrap();
    assert_eq!(extract[0].path, archive);
    assert_eq!(menu.selection(20).unwrap()[0].path, marked);
    assert!(menu.extract_selection(0, &archive).is_err());
    assert!(menu.extract_selection(21, &archive).is_err());
    assert!(validate_sources(Some(&extract), std::slice::from_ref(&marked)).is_err());
    root.replace_file(&archive, "replacement archive");
    assert!(validate_sources(Some(&extract), std::slice::from_ref(&archive)).is_err());
    menu.snapshot.lock().unwrap().id = 0;
    assert!(menu.extract_selection(20, &archive).is_err());
}

// Issue 397: several marked archives each validate and each extract takes only its own captured item.
#[test]
fn several_marked_archives_validate_together_and_extract_one_by_one() {
    let root = TestDir::new("menu-extract-several");
    let a = root.file("a.zip", "first archive");
    let b = root.file("b.tar.gz", "second archive");
    let other = root.file("c.zip", "never marked");
    let mut snapshot = Snapshot::default();
    let reply = snapshot.handle_request(
        r#"{"op":"snapshot","id":30}"#,
        vec![a.to_str().unwrap().into(), b.to_str().unwrap().into()],
        a.to_str(),
        &Registry::default(),
        &Cancellation::default(),
    );
    assert!(field_bool(&reply, "ok"), "{reply}");
    let reply = snapshot.handle(r#"{"op":"validate","id":30,"action":"extract"}"#, vec![]);
    assert!(field_bool(&reply, "ok"), "{reply}");
    assert_eq!(
        crate::json::field_str_array(&reply, "paths"),
        [a.to_str().unwrap(), b.to_str().unwrap()]
    );
    let (menu, _rx) = super::tests::unstarted_menu();
    *menu.snapshot.lock().unwrap() = snapshot;
    for path in [&a, &b] {
        let one = menu.extract_selection(30, path).unwrap();
        assert_eq!(one.len(), 1);
        assert!(validate_sources(Some(&one), std::slice::from_ref(path)).is_ok());
    }
    assert!(
        menu.extract_selection(30, &other).is_err(),
        "an unmarked archive is not this menu's"
    );
    root.replace_file(&b, "replaced archive");
    let reply = menu
        .snapshot
        .lock()
        .unwrap()
        .handle(r#"{"op":"validate","id":30,"action":"extract"}"#, vec![]);
    assert!(
        !field_bool(&reply, "ok"),
        "a replaced archive fails the whole validation: {reply}"
    );
}
