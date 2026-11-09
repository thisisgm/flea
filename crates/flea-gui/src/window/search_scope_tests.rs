//! EY475: scope changes keep the original folder, query and focus after a Home result lands.
use super::{
    grid_tests::answer_at,
    tests::{edit, frame, key, open},
};
use gpui::TestAppContext;

#[gpui::test]
fn ey475_tab_rescopes_after_home_results_without_losing_query_or_focus(cx: &mut TestAppContext) {
    use flea::{json, ui_logic_file_search::Scope};
    let (handle, mut read) = open(cx);
    let home = std::env::var("HOME").expect("HOME");
    let notes = format!("{home}/Notes");
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().path = notes.clone()
    });
    key(cx, handle, "f", Some("f"));
    for letter in ["k", "i", "w", "i"] {
        key(cx, handle, letter, Some(letter));
    }
    while read().is_some() {}
    let mut generation = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().search.owner.session.gen as usize
    });
    for (turn, (scope, path, names)) in [
        (
            Scope::Home,
            home.as_str(),
            vec![
                "Notes/kiwi-a.txt",
                "Notes/sub/kiwi-b.txt",
                "Other/kiwi-c.txt",
            ],
        ),
        (
            Scope::Folder,
            notes.as_str(),
            vec!["kiwi-a.txt", "sub/kiwi-b.txt"],
        ),
        (
            Scope::Home,
            home.as_str(),
            vec![
                "Notes/kiwi-a.txt",
                "Notes/sub/kiwi-b.txt",
                "Other/kiwi-c.txt",
            ],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        answer_at(cx, handle, generation, path, &names);
        frame(cx, handle);
        edit(cx, handle, |shell, window, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.path, path, "landed rows use their scope root");
            assert_eq!(host.search.owner.session.scope, scope);
            assert_eq!(host.search.owner.session.query, "kiwi");
            assert!(host.search.owner.typing());
            assert!(shell.focus.is_focused(window));
            let place = if scope == Scope::Folder {
                "~/Notes"
            } else {
                "~"
            };
            assert_eq!(
                host.search
                    .owner
                    .words(&shell.locale.catalog, &host.path, &home, false)
                    .scope,
                shell.locale.catalog.tr_args("search.scope.in", &[place])
            );
        });
        if turn == 2 {
            break;
        }
        while read().is_some() {}
        key(cx, handle, "tab", None);
        let opened = read().expect("Tab opens the other scope");
        assert_eq!(
            json::field_str(&opened, "c").as_deref(),
            Some("searchopen"),
            "{opened}"
        );
        let next = if scope == Scope::Home { &notes } else { &home };
        assert_eq!(
            json::field_str(&opened, "path").as_deref(),
            Some(next.as_str())
        );
        assert_eq!(
            json::field_str(&opened, "folder").as_deref(),
            Some(notes.as_str())
        );
        let query = read().expect("Tab resends the query");
        assert_eq!(json::field_str(&query, "query").as_deref(), Some("kiwi"));
        generation = json::field_usize(&query, "gen").expect("query generation");
        edit(cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            let place = if scope == Scope::Home { "~/Notes" } else { "~" };
            assert_eq!(
                host.search
                    .owner
                    .words(&shell.locale.catalog, &host.path, &home, false)
                    .scope,
                shell.locale.catalog.tr_args("search.scope.in", &[place]),
                "new scope label while old rows remain"
            );
        });
    }
}
