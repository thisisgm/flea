//! Mounted query input uses the platform handler, the real Shell route and the backend queue.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use gpui::{
    px, AppContext, Context, PlatformInputHandler, PlatformWindow, TestAppContext, Window,
    WindowHandle,
};

type Handle = WindowHandle<Root>;

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
        assert!(shell.filter_action("filter", window, cx));
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
fn i50_query_filter_composed_commit_once(cx: &mut TestAppContext) {
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
        query.contains("\"query\":\"日本\"") && query.contains("\"id\":1"),
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

fn answer(cx: &mut TestAppContext, handle: Handle, id: u64, query: &str, rows: &str) {
    let close = if rows == "[]" { "[9]" } else { "[]" };
    let line = format!(
        r#"{{"t":"filtered","id":{id},"query":"{query}","n":1000,"rows":{rows},"close":{close},"ms":0.01,"listing":2}}"#
    );
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(&line), window, cx)
    });
}

#[gpui::test]
fn i50_query_filter_preedit_drawn(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_and_mark_text_in_range(None, "a日本b", Some(2..2));
    frame(cx, handle);
    assert_eq!(input.marked_text_range(), Some(0..4));
    assert_eq!(input.selected_text_range(false).unwrap().range, 2..2);
    let candidate = input.bounds_for_range(2..2).expect("actual filter caret");
    edit(cx, handle, |shell, _, _| {
        let filter = &shell.panes[0].as_ref().unwrap().filter;
        let strip = filter.strip.borrow();
        assert_eq!(strip.text.query.as_ref(), "a日本b");
        let caret = strip.laid.unwrap().caret.unwrap();
        assert!((candidate.left() - caret.left()).abs() <= gpui::px(1.));
        assert_eq!(
            (candidate.top(), candidate.bottom()),
            (caret.top(), caret.bottom())
        );
        assert!(strip.query.marked.unwrap().size.width > gpui::px(0.));
        assert_eq!(filter.owner.query(), "");
    });
    assert_eq!(read(), None, "preedit never asks the matcher");
}

#[gpui::test]
fn i50_query_filter_ime_escape_first(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_text_in_range(None, "a");
    answer(cx, handle, 1, "a", "[3]");
    while read().is_some() {}
    input.replace_and_mark_text_in_range(None, "かな", Some(2..2));
    for name in ["down", "left", "home", "enter", "tab"] {
        key(cx, handle, name, false);
    }
    key(cx, handle, "a", true);
    assert_eq!(input.marked_text_range(), Some(1..3));
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.pane.cursor, 3);
        assert!(host.selected.is_empty() && host.filter.owner.typing());
        assert_eq!(host.filter.owner.query(), "a");
    });
    assert_eq!(
        read(),
        None,
        "candidate keys must not open the single exact row or send a query"
    );
    key(cx, handle, "escape", false);
    assert_eq!(input.marked_text_range(), None);
    edit(cx, handle, |shell, _, _| {
        let owner = &shell.panes[0].as_ref().unwrap().filter.owner;
        assert!(owner.typing());
        assert_eq!(owner.entry.borrow().text, "a");
    });
    key(cx, handle, "escape", false);
    edit(cx, handle, |shell, _, _| {
        assert!(!shell.panes[0].as_ref().unwrap().filter.owner.up())
    });
    input.replace_text_in_range(None, "late");
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().filter.owner.query(), "")
    });
}

#[gpui::test]
fn i50_query_filter_ctrl_a_scope(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut old = input(cx, handle);
    old.replace_text_in_range(None, "a");
    answer(cx, handle, 1, "a", "[3,5]");
    while read().is_some() {}
    key(cx, handle, "tab", false);
    assert_eq!(
        read(),
        None,
        "Tab stays inert until filter26 supplies its recursive producer"
    );
    // Ctrl+A on the typed line stays on it (EY-115); after a cursor key the list's Ctrl+A marks the matches.
    key(cx, handle, "a", true);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.selected.is_empty() && host.filter.owner.typing());
    });
    key(cx, handle, "home", false);
    key(cx, handle, "a", true);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, [3, 5]);
        assert!(!host.filter.owner.typing());
        assert_eq!(host.filter.owner.query(), "a");
    });
    old.replace_text_in_range(None, "late");
    edit(cx, handle, |shell, window, cx| {
        assert!(shell.filter_action("filter", window, cx));
    });
    frame(cx, handle);
    old.replace_text_in_range(None, "retained callback");
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().filter.owner.query(), "a")
    });
    assert_eq!(read(), None);
}

#[gpui::test]
fn i50_query_filter_listing_navigation_preserved(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    key(cx, handle, "j", false);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!((host.filter.owner.query(), host.pane.cursor), ("j", 0));
    });
    answer(cx, handle, 1, "j", "[3,5]");
    while read().is_some() {}
    key(cx, handle, "down", false);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.filter.owner.typing());
        assert_eq!(
            host.pane.cursor, 5,
            "ordinary arrows commit and move the drawn rows"
        );
    });
    key(cx, handle, "k", false);
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.panes[0].as_ref().unwrap().pane.cursor, 3)
    });
    assert_eq!(read(), None);
}

#[gpui::test]
fn i50_query_filter_stale_result_generation(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_text_in_range(None, "first");
    input.replace_text_in_range(Some(0..5), "second");
    answer(cx, handle, 1, "first", "[7]");
    edit(cx, handle, |shell, _, _| {
        // M3's filter lands every awaited keystroke in order, so the earlier answer draws until the newer one.
        let filter = &shell.panes[0].as_ref().unwrap().filter;
        assert_eq!(filter.shown.as_deref(), Some([7].as_slice()));
    });
    answer(cx, handle, 2, "second", "[]");
    edit(cx, handle, |shell, _, _| {
        let filter = &shell.panes[0].as_ref().unwrap().filter;
        assert_eq!(
            filter.shown.as_deref(),
            Some([9].as_slice()),
            "no exact row draws the accepted close answer"
        );
    });
    input.replace_and_mark_text_in_range(None, "draft", None);
    answer(cx, handle, 1, "first", "[7]");
    edit(cx, handle, |shell, _, _| {
        let filter = &shell.panes[0].as_ref().unwrap().filter;
        assert_eq!(filter.owner.query(), "second");
        assert_eq!(filter.shown.as_deref(), Some([9].as_slice()));
    });
    while read().is_some() {}
    key(cx, handle, "escape", false);
    key(cx, handle, "escape", false);
    answer(cx, handle, 2, "second", "[8]");
    edit(cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().filter.shown.is_none())
    });
    assert_eq!(read(), None);
}

#[gpui::test]
fn i50_query_filter_supplementary_commit_once(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_and_mark_text_in_range(None, "a𝄞b", Some(3..3));
    assert_eq!(input.marked_text_range(), Some(0..4));
    assert_eq!(input.selected_text_range(false).unwrap().range, 3..3);
    input.replace_text_in_range(None, "a𝄞b");
    let mut adjusted = None;
    assert_eq!(
        input.text_for_range(2..2, &mut adjusted).as_deref(),
        Some("𝄞")
    );
    assert_eq!(adjusted, Some(1..3));
    input.replace_text_in_range(Some(2..2), "日");
    let sent: Vec<_> = std::iter::from_fn(&mut read).collect();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert!(sent[0].contains("\"id\":1") && sent[1].contains("\"id\":2"));
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().filter.owner.query(),
            "a日b"
        )
    });
}

#[gpui::test]
fn i50_query_filter_platform_control_text_refused(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let mut input = input(cx, handle);
    input.replace_text_in_range(None, "a\nb");
    input.replace_and_mark_text_in_range(None, "c\u{7}", Some(2..2));
    input.replace_text_in_range(None, "d\u{85}");
    cx.run_until_parked();
    assert_eq!(read(), None, "control text never reaches the filter query");
    edit(cx, handle, |shell, _, _| {
        let filter = &shell.panes[0].as_ref().unwrap().filter.owner;
        assert_eq!(filter.query(), "");
        assert_eq!(filter.entry.borrow().text, "");
    });
    input.replace_text_in_range(None, "ok");
    let sent = read().expect("clean text still commits");
    assert!(sent.contains("\"query\":\"ok\""), "{sent}");
}

// A query wider than its field scrolls; in an Arabic window its caret, at the end or the start, stays inside the mirrored field.
#[gpui::test]
fn i50_query_filter_scrolled_query_stays_in_the_mirrored_field(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    while read().is_some() {}
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../src/keys/fixtures/keycaps");
    let catalog = flea::i18n::I18n::load(flea::i18n::Paths {
        omarchy: fixture.clone(),
        language_file: fixture.join("language-ar"),
        catalogs: vec![fixture.join("catalogs")],
    });
    edit(cx, handle, |shell, _, cx| {
        let catalog = std::sync::Arc::new(catalog);
        let locale = crate::window::locale::Snapshot {
            revision: 2,
            catalog,
        };
        shell.apply_locale(locale, cx)
    });
    frame(cx, handle);
    assert!(edit(cx, handle, |_, window, _| window.layout_mirrored()));
    let mut input = input(cx, handle);
    let long = "w".repeat(400);
    input.replace_text_in_range(None, &long);
    for at in [400, 0] {
        edit(cx, handle, |shell, _, _| {
            let owner = &shell.panes[0].as_ref().unwrap().filter.owner;
            owner.entry.borrow_mut().move_to(at);
        });
        frame(cx, handle);
        let caret = input.bounds_for_range(at..at).expect("filter caret");
        edit(cx, handle, |shell, window, _| {
            let filter = &shell.panes[0].as_ref().unwrap().filter;
            let strip = filter.strip.borrow();
            let field = strip.query.bounds.get().expect("the query is placed");
            let laid = strip.laid.unwrap();
            assert!(
                f32::from(laid.query.size.width) > 2. * f32::from(field.size.width),
                "fixture must scroll: {laid:?} {field:?}"
            );
            let width = window.viewport_size().width;
            let (start, end) = (width - field.right(), width - field.left());
            assert!(
                caret.left() >= start - px(1.) && caret.right() <= end + px(1.),
                "caret at {at}: {caret:?} left the mirrored field {start:?}..{end:?}"
            );
        });
    }
}

#[path = "../../strips/i50_query_columns_tests.rs"]
mod columns;
