//! Mounted commit keys preserve the IME candidate and its backend destination.
use super::*;

#[gpui::test]
fn i50_ime_root_find_tab_commits(cx: &mut TestAppContext) {
    use flea::{json, ui_logic_file_search::Scope};
    let (handle, mut read) = mounted(cx);
    let home = std::env::var("HOME").expect("HOME");
    let folder = format!("{}/i50-ime-scope", home.trim_end_matches('/'));
    assert_ne!(folder, home, "the two scopes have distinct roots");
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().path = folder.clone();
    });
    open(cx, handle, Entry::Find);
    compose(cx, handle, "日本");
    while read().is_some() {}
    let (scope, next_scope) = edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        (session.scope, session.next_scope())
    });
    assert_ne!(scope, next_scope, "Tab has another scope to move to");
    assert_eq!((scope, next_scope), (Scope::Folder, Scope::Home));
    cx.simulate_keystrokes(handle.into(), "tab");
    frame(cx, handle);
    assert_eq!(owner(cx, handle), None, "Find's Tab commits the candidate");
    let sent = std::iter::from_fn(&mut read).collect::<Vec<_>>();
    let queries = sent
        .iter()
        .filter(|line| json::field_str(line, "c").as_deref() == Some("searchquery"))
        .collect::<Vec<_>>();
    assert_eq!(queries.len(), 1, "Tab sends one query: {sent:?}");
    assert_eq!(
        json::field_str(queries[0], "query").as_deref(),
        Some("日本"),
        "the query sent is the committed candidate: {sent:?}"
    );
    // searchquery inherits the preceding searchopen's scope in this protocol.
    let opens = sent
        .iter()
        .filter(|line| json::field_str(line, "c").as_deref() == Some("searchopen"))
        .collect::<Vec<_>>();
    assert_eq!(opens.len(), 1, "Tab opens one new scope: {sent:?}");
    assert_eq!(
        json::field_str(opens[0], "path").as_deref(),
        Some(home.as_str()),
        "the query walks the next scope's root: {sent:?}"
    );
    assert_eq!(
        json::field_str(opens[0], "folder").as_deref(),
        Some(folder.as_str()),
        "the search keeps its originating folder: {sent:?}"
    );
    let commands = sent
        .iter()
        .filter_map(|line| json::field_str(line, "c"))
        .filter(|command| matches!(command.as_str(), "searchopen" | "searchquery"))
        .collect::<Vec<_>>();
    assert_eq!(commands, ["searchopen", "searchquery"], "{sent:?}");
    edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        assert_eq!(session.query, "日本");
        assert_eq!(
            session.scope, next_scope,
            "Tab moves Find to its next scope"
        );
    });
}

#[gpui::test]
fn i50_ime_root_enter_exactly_once(cx: &mut TestAppContext) {
    let (handle, mut read) = mounted(cx);
    open(cx, handle, Entry::Name);
    while read().is_some() {}
    compose(cx, handle, "に");
    cx.simulate_keystrokes(handle.into(), "enter");
    frame(cx, handle);
    assert_eq!(
        read(),
        None,
        "a candidate Enter belongs to the input method"
    );
    assert!(
        edit(cx, handle, |shell, _, _| shell.rename.edit.is_some()),
        "the name editor stays open under its composition"
    );
    commit(cx, handle, "日本");
    cx.simulate_keystrokes(handle.into(), "enter");
    frame(cx, handle);
    let sent = std::iter::from_fn(&mut read).collect::<Vec<_>>();
    assert_eq!(
        sent.iter().filter(|line| line.contains("日本")).count(),
        1,
        "the committed name is sent exactly once: {sent:?}"
    );
}
