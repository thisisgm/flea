//! The mounted query's boundaries: clearing it sends the empty query in a newer generation, control text sends nothing.
use super::*;

#[gpui::test]
fn i50_query_empty_commit_advances_generation(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut native = input(cx, handle);
    native.replace_text_in_range(None, "abc");
    cx.run_until_parked();
    let first = read().expect("the committed query is sent");
    assert!(
        first.contains("\"query\":\"abc\"") && first.contains("\"gen\":1"),
        "{first}"
    );
    assert_eq!(read(), None);

    key(cx, handle, "a", true);
    key(cx, handle, "backspace", false);
    let cleared = read().expect("clearing the field sends an empty query");
    assert!(
        cleared.contains("\"query\":\"\"") && cleared.contains("\"gen\":2"),
        "{cleared}"
    );
    assert_eq!(read(), None, "clearing sends exactly one new query");
    edit(cx, handle, |shell, _, _| {
        let search = &shell.panes[0].as_ref().unwrap().search.owner;
        assert!(search.typing());
        assert_eq!(search.session.query, "");
        assert_eq!(search.session.gen, 2);
    });
}

#[gpui::test]
fn i50_query_platform_empty_commit_rejects_old_results(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut native = input(cx, handle);
    native.replace_text_in_range(None, "abc");
    native.replace_text_in_range(Some(0..3), "");
    let sent: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert!(sent[1].contains("\"query\":\"\"") && sent[1].contains("\"gen\":2"));
    edit(cx, handle, |shell, window, cx| {
        shell.search_line(
            0,
            r#"{"t":"searched","n":99,"scanned":99,"ms":1,"gen":1}"#,
            window,
            cx,
        );
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            (
                host.search.owner.session.gen,
                host.search.owner.session.total
            ),
            (2, 0)
        );
        assert_eq!(host.search.owner.session.query, "");
        assert_eq!(host.pane.swap.session.query, "");
        assert_eq!(host.pane.swap.session.generation, 2);
    });
}

#[gpui::test]
fn i50_query_mounted_contents_composition(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    edit(cx, handle, |shell, _, _| {
        assert!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .search
                .owner
                .session
                .wire
                .contents
        );
    });
    let mut native = input(cx, handle);
    native.replace_and_mark_text_in_range(None, "日本", Some(2..2));
    assert_eq!(read(), None);
    key(cx, handle, "g", true);
    let sent: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(
        sent.iter()
            .filter(|line| line.contains("searchcontents"))
            .count(),
        1,
        "{sent:?}"
    );
    assert!(sent
        .iter()
        .any(|line| line.contains("\"query\":\"日本\"") && line.contains("\"gen\":1")));
    native.replace_text_in_range(None, "retained");
    assert_eq!(
        read(),
        None,
        "scope change invalidates the old platform handler"
    );
    frame(cx, handle);
    let mut native = input(cx, handle);
    native.replace_and_mark_text_in_range(None, "語", Some(1..1));
    frame(cx, handle);
    assert!(native.bounds_for_range(3..3).is_some());
    native.replace_text_in_range(None, "語");
    let sent: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert!(
        sent[0].contains("searchcontents")
            && sent[0].contains("\"gen\":2")
            && sent[0].contains("日本語")
    );
}

#[gpui::test]
fn i50_query_platform_control_text_refused(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut native = input(cx, handle);
    native.replace_text_in_range(None, "a\nb");
    native.replace_and_mark_text_in_range(None, "c\u{7}", Some(2..2));
    native.replace_text_in_range(None, "d\u{85}");
    cx.run_until_parked();
    assert_eq!(read(), None, "control text never reaches the search query");
    edit(cx, handle, |shell, _, _| {
        let search = &shell.panes[0].as_ref().unwrap().search.owner;
        assert_eq!(search.session.query, "");
        assert_eq!(search.entry.borrow().text, "");
    });
    native.replace_text_in_range(None, "ok");
    let sent = read().expect("clean text still commits");
    assert!(sent.contains("\"query\":\"ok\""), "{sent}");
}
