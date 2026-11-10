//! Enabled search scopes retain one IME commit while changing producers.
use super::*;

#[gpui::test]
fn i50_query_enabled_capability_fixture(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().search.owner.session.wire =
            flea::ui_logic_file_search::Wire {
                everywhere: true,
                contents: true,
            };
    });
    key(cx, handle, "tab", false);
    let mut input = input(cx, handle);
    input.replace_and_mark_text_in_range(None, "日本", Some(2..2));
    key(cx, handle, "tab", false);
    let sent: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(
        sent.iter()
            .filter(|line| line.contains("searchquery"))
            .count(),
        1,
        "{sent:?}"
    );
    edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        assert_eq!(
            (session.scope, session.gen),
            (flea::ui_logic_file_search::Scope::Everywhere, 1)
        );
    });
    key(cx, handle, "tab", false);
    key(cx, handle, "g", true);
    edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        assert_eq!(
            (session.kind, session.gen),
            (flea::ui_logic_file_search::Kind::Contents, 3)
        );
        assert_eq!(session.query, "日本");
    });
}
