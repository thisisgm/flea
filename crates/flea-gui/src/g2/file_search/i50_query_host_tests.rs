//! Mounted query input uses the platform handler, the real Shell route and the backend queue.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{
    AppContext, Context, PlatformInputHandler, PlatformWindow, TestAppContext, Window, WindowHandle,
};

type Handle = WindowHandle<Root>;

#[path = "i50_query_empty_tests.rs"]
mod empty;

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| change(shell, window, cx))
        })
        .unwrap()
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn open(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, read) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        assert!(shell.search_action("search", window, cx));
    });
    frame(cx, handle);
    (handle, read)
}

fn input(cx: &mut TestAppContext, handle: Handle) -> PlatformInputHandler {
    let mut native = cx.test_window(handle.into());
    let mut input = native
        .take_input_handler()
        .expect("query must register platform input");
    assert!(
        input.selected_text_range(false).is_some(),
        "query must own live platform text input"
    );
    input
}

#[gpui::test]
fn i50_query_composed_commit_once(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_and_mark_text_in_range(None, "に", Some(1..1));
    input.replace_and_mark_text_in_range(None, "日本", Some(2..2));
    cx.run_until_parked();
    assert_eq!(read(), None, "preedit must not start backend generations");
    input.replace_text_in_range(None, "日本");
    cx.run_until_parked();
    let query = read().expect("one committed query");
    assert!(
        query.contains("\"query\":\"日本\"") && query.contains("\"gen\":1"),
        "{query}"
    );
    assert_eq!(read(), None, "one IME commit must send exactly once");
    input.unmark_text();
    cx.run_until_parked();
    assert_eq!(read(), None, "unmark after commit must not resend");
}
fn key(cx: &mut TestAppContext, handle: Handle, name: &str, control: bool) {
    edit(cx, handle, |shell, window, cx| {
        shell.key(
            &gpui::KeyDownEvent {
                keystroke: gpui::Keystroke {
                    key: name.into(),
                    key_char: (!control && name.chars().count() == 1).then(|| name.into()),
                    modifiers: gpui::Modifiers {
                        control,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                is_held: false,
                prefer_character_input: false,
            },
            window,
            cx,
        )
    });
    cx.run_until_parked();
}

#[gpui::test]
fn i50_query_preedit_drawn(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_and_mark_text_in_range(None, "a日本b", Some(2..2));
    frame(cx, handle);
    assert_eq!(input.marked_text_range(), Some(0..4));
    assert_eq!(input.selected_text_range(false).unwrap().range, 2..2);
    let candidate = input
        .bounds_for_range(2..2)
        .expect("caret candidate bounds");
    edit(cx, handle, |shell, _, _| {
        let search = &shell.panes[0].as_ref().unwrap().search;
        let strip = search.strip.borrow();
        assert_eq!(search.owner.entry.borrow().text, "a日本b");
        assert!(strip.prepared.as_ref().unwrap().query.line().width > gpui::px(0.));
        let caret = strip.edit.caret.expect("real composing caret");
        assert!((candidate.left() - caret.left()).abs() <= gpui::px(1.));
        assert_eq!(
            (candidate.top(), candidate.bottom()),
            (caret.top(), caret.bottom())
        );
        assert!(strip.edit.marked.expect("preedit underline").size.width > gpui::px(0.));
        assert_eq!(
            search.owner.session.query, "",
            "preedit never becomes the search query"
        );
    });
    assert_eq!(read(), None);
}

#[gpui::test]
fn i50_query_ime_escape_first(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_and_mark_text_in_range(None, "かな", Some(2..2));
    for name in ["down", "left", "enter"] {
        key(cx, handle, name, false);
    }
    assert_eq!(input.marked_text_range(), Some(0..2));
    edit(cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().search.owner.typing())
    });
    key(cx, handle, "escape", false);
    assert_eq!(input.marked_text_range(), None);
    edit(cx, handle, |shell, _, _| {
        let search = &shell.panes[0].as_ref().unwrap().search.owner;
        assert!(search.typing(), "first Escape cancels composition only");
        assert_eq!(search.entry.borrow().text, "");
    });
    assert_eq!(read(), None);
    key(cx, handle, "escape", false);
    edit(cx, handle, |shell, _, _| {
        assert!(!shell.panes[0].as_ref().unwrap().search.owner.typing())
    });
}

#[gpui::test]
fn i50_query_ctrl_a_scope(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_text_in_range(None, "original");
    cx.run_until_parked();
    while read().is_some() {}
    key(cx, handle, "a", true);
    assert_eq!(input.selected_text_range(false).unwrap().range, 0..8);
    input.replace_and_mark_text_in_range(None, "日本", Some(2..2));
    let next_scope = edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .search
            .owner
            .session
            .next_scope()
    });
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
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.search.owner.session.query, "日本");
        assert!(
            host.selected.is_empty(),
            "Find Ctrl+A selects text, never listing rows"
        );
        assert_eq!(host.search.owner.session.scope, next_scope);
    });
    input.replace_text_in_range(None, "late composition");
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.query,
            "日本"
        );
    });
    let prior_kind = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().search.owner.session.kind
    });
    key(cx, handle, "g", true);
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.kind,
            match prior_kind {
                flea::ui_logic_file_search::Kind::Names =>
                    flea::ui_logic_file_search::Kind::Contents,
                flea::ui_logic_file_search::Kind::Contents =>
                    flea::ui_logic_file_search::Kind::Names,
            },
            "the mounted contents capability stays enabled"
        )
    });
}

#[gpui::test]
fn i50_query_listing_navigation_preserved(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let before = edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().pane.cursor
    });
    let mut input = input(cx, handle);
    input.replace_text_in_range(None, "j");
    cx.run_until_parked();
    assert_eq!(
        edit(cx, handle, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .cursor),
        before
    );
    key(cx, handle, "enter", false);
    input.replace_text_in_range(None, "late");
    cx.run_until_parked();
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.query,
            "j"
        )
    });
    edit(cx, handle, |shell, window, cx| {
        shell.search_update(
            0,
            None,
            |search, host| {
                search.leave(host);
                search.landed(host);
            },
            window,
            cx,
        )
    });
    key(cx, handle, "j", false);
    assert_eq!(
        edit(cx, handle, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .cursor),
        before + 1,
        "listing j stays navigation"
    );
}

#[gpui::test]
fn i50_query_stale_result_generation(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_text_in_range(None, "first");
    input.replace_text_in_range(Some(0..5), "second");
    cx.run_until_parked();
    edit(cx, handle, |shell, window, cx| {
        shell.search_line(
            0,
            r#"{"t":"searched","n":99,"scanned":99,"ms":1,"cancelled":false,"gen":1,"walking":false}"#,
            window,
            cx,
        );
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.total,
            0
        );
        shell.search_line(
            0,
            r#"{"t":"searched","n":2,"scanned":99,"ms":1,"cancelled":false,"gen":2,"walking":false}"#,
            window,
            cx,
        );
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.total,
            2
        );
        shell.search_update(
            0,
            None,
            |search, host| {
                search.leave(host);
                search.landed(host);
            },
            window,
            cx,
        );
        assert!(shell.search_action("search", window, cx));
    });
    frame(cx, handle);
    input.replace_text_in_range(None, "retained callback");
    cx.run_until_parked();
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.query,
            "",
            "retained old handler never edits a reopened field"
        )
    });
}

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

#[gpui::test]
fn i50_query_utf16_supplementary_commit(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_text_in_range(None, "a𝄞b");
    assert_eq!(input.selected_text_range(false).unwrap().range, 4..4);
    let mut adjusted = None;
    assert_eq!(
        input.text_for_range(2..2, &mut adjusted).as_deref(),
        Some("𝄞")
    );
    assert_eq!(adjusted, Some(1..3));
    input.replace_text_in_range(Some(2..2), "日");
    cx.run_until_parked();
    edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        assert_eq!((session.query.as_str(), session.gen), ("a日b", 2));
    });
    let sent: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(
        sent.iter()
            .filter(|line| line.contains("searchquery"))
            .count(),
        2
    );
}
