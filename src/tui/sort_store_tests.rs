use crate::{
    backend::testdir::TestDir,
    jsondoc,
    tui::{action_key_tests::listing, actions, wire},
    uistore::Store,
};

#[test]
fn new_sorts_are_session_orders_and_leave_shared_sort_settings_valid() {
    for action in [
        "sortBirth",
        "sortBirthReverse",
        "sortExtension",
        "sortRandom",
    ] {
        let root = TestDir::new("tui17-sort-store");
        let store = Store::at(&root.join("state"), &root.join("config"));
        assert!(store.file().is_absolute() && store.file().starts_with(root.path()));
        store
            .update(&jsondoc::parse(r#"{"sort":{"key":"size","reverse":false}}"#).unwrap())
            .unwrap();
        let (mut w, reader) = wire::echo_wire();
        let mut m = listing(&mut w, "yazi");
        m.store = Some(store);
        actions::act(&mut m, action, &mut w).unwrap();
        assert!(m.error.is_empty(), "{action}: {}", m.error);
        let saved = m.store.as_ref().unwrap().read();
        assert_eq!(
            saved.get("sort").unwrap().get("key").unwrap().as_str(),
            Some("size")
        );
        actions::act(&mut m, "sortName", &mut w).unwrap();
        let saved = m.store.as_ref().unwrap().read();
        assert_eq!(
            saved.get("sort").unwrap().get("key").unwrap().as_str(),
            Some("name")
        );
        assert!(m.error.is_empty());
        wire::finish(w, reader);
    }
}
