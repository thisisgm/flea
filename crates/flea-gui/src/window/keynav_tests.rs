//! Keyboard parity through the mounted Root, with input dispatched to its real focus owners.
use super::*;
use gpui::VisualTestContext;

#[path = "keynav_focus_tests.rs"]
mod controls;

fn listing(cx: &mut TestAppContext) -> Handle {
    let handle = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.launch.settings.data = flea::uischema::defaults();
        shell.set_view(View::List, window, cx);
        shell.mount_rail(cx);
        window.focus(&shell.focus, cx);
    });
    frame(cx, handle);
    handle
}

fn key(cx: &mut TestAppContext, handle: Handle, stroke: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(stroke);
    frame(cx, handle);
}

#[gpui::test]
fn ey142_forward_tab_enters_the_first_live_chrome_glyph(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "tab");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("rail"));
    key(cx, handle, "tab");
    assert_eq!(
        call(cx, handle, "focusStop"),
        Reply::value("chrome:arrow-up")
    );
    edit(cx, handle, |shell, _, cx| {
        let chrome = shell.chrome.view.as_ref().unwrap().read(cx);
        assert!(chrome.frame.borrow().controls[1].focused);
        assert!(
            !shell
                .rail
                .as_ref()
                .unwrap()
                .places
                .read(cx)
                .model
                .borrow()
                .focused
        );
    });
    key(cx, handle, "tab");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
}

#[gpui::test]
fn ey159_reverse_tab_enters_chrome_before_rail(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "shift-tab");
    assert_eq!(
        call(cx, handle, "focusStop"),
        Reply::value("chrome:sliders")
    );
    edit(cx, handle, |shell, _, cx| {
        assert!(
            !shell
                .rail
                .as_ref()
                .unwrap()
                .places
                .read(cx)
                .model
                .borrow()
                .focused
        );
    });
    key(cx, handle, "shift-tab");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("rail"));
    key(cx, handle, "shift-tab");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
}

#[gpui::test]
fn ey247_escape_from_a_panel_stop_keeps_the_listing_selection(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "t");
    for (tabs, stop) in [(1, "rail"), (2, "chrome"), (3, "strip")] {
        key(cx, handle, "ctrl-a");
        assert_eq!(call(cx, handle, "selectionCount"), Reply::value("8"));
        for _ in 0..tabs {
            key(cx, handle, "tab");
        }
        assert_eq!(call(cx, handle, "focusView"), Reply::value(stop));
        key(cx, handle, "escape");
        assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
        assert_eq!(
            call(cx, handle, "selectionCount"),
            Reply::value("8"),
            "Escape from the {stop} cleared the listing selection"
        );
    }
}

#[gpui::test]
fn ey143_columns_wrap_only_when_enabled(cx: &mut TestAppContext) {
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let (backend, _) = Backend::test_channel();
            let mut shell = shell_with(backend, cx);
            shell.launch.settings.data = flea::uischema::defaults();
            let host = shell.panes[0].as_mut().unwrap();
            host.backend = None;
            assert!(host
                .pane
                .begin("/files".into(), Default::default(), host.pane.now()));
            host.pane
                .receive(flea::protocol::Reply::Listed {
                    path: "/files".into(),
                    total: 8,
                })
                .unwrap();
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: (0..8)
                        .map(|at| flea::protocol::RowFacts {
                            name: format!("file-{at}.txt"),
                            ..Default::default()
                        })
                        .collect(),
                })
                .unwrap();
            shell.view = View::Columns;
            shell.sync_columns(window, cx);
            crate::window::columns_tests::host::settle(&mut shell, window, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    edit(cx, handle, |shell, window, cx| {
        let patch = flea::jsondoc::parse(r#"{"wrapAtEnds":true}"#).unwrap();
        shell.change_settings(patch, window, cx);
        shell.set_view(View::Columns, window, cx);
    });
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().drawn_view,
            Some(View::Columns)
        );
        let host = shell.panes[0].as_ref().unwrap();
        assert!(
            host.columns_key_frame(shell.grows[0])
                .is_some_and(|frame| frame.total == 8),
            "Columns keys measure the painted track, not a List frame"
        );
    });
    key(cx, handle, "end");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("7"));
    key(cx, handle, "j");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("0"));
    key(cx, handle, "k");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("7"));
    edit(cx, handle, |shell, window, cx| {
        let patch = flea::jsondoc::parse(r#"{"wrapAtEnds":false}"#).unwrap();
        shell.change_settings(patch, window, cx);
    });
    key(cx, handle, "j");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("7"));
}

#[gpui::test]
fn ey144_escape_cancels_pairs_before_escape_up(cx: &mut TestAppContext) {
    let handle = listing(cx);
    let (backend, mut requests) = Backend::test_lines();
    edit(cx, handle, |shell, window, cx| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend);
        let patch = flea::jsondoc::parse(r#"{"escapeUp":true}"#).unwrap();
        shell.change_settings(patch, window, cx);
    });
    for (preset, arm) in [("default", "d"), ("vim", "y"), ("vim", "g")] {
        edit(cx, handle, |shell, window, cx| {
            let patch = Json::Obj(vec![("keys".into(), Json::Str(preset.into()))]);
            shell.change_settings(patch, window, cx);
        });
        key(cx, handle, arm);
        key(cx, handle, "escape");
        edit(cx, handle, |shell, _, _| {
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(host.pane.place(), "/files", "{preset} {arm}");
            assert!(!host.pane.busy(), "{preset} {arm} must only disarm");
        });
        while let Some(line) = requests() {
            assert_ne!(flea::json::field_str(&line, "c").as_deref(), Some("list"));
        }
    }
    key(cx, handle, "escape");
    assert!(std::iter::from_fn(&mut requests)
        .any(|line| flea::json::field_str(&line, "c").as_deref() == Some("list")));
}

#[gpui::test]
fn ey145_down_moves_search_results_while_letters_keep_typing(cx: &mut TestAppContext) {
    let handle = listing(cx);
    let (backend, _requests) = Backend::test_lines();
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().backend = Some(backend)
    });
    key(cx, handle, "f a");
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            Backend::test_reply_line(r#"{"t":"listed","path":"/files","n":2,"gen":1}"#),
            window,
            cx,
        );
        shell.receive(
            0,
            BackendEvent::Session {
                generation: 1,
                reply: flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 4,
                    rows: ["A", "alpha.txt"]
                        .into_iter()
                        .map(|name| flea::protocol::RowFacts {
                            name: name.into(),
                            ..Default::default()
                        })
                        .collect(),
                },
            },
            window,
            cx,
        );
        shell.receive(0, Backend::test_reply_line(r#"{"t":"searched","n":2,"scanned":2,"ms":1,"cancelled":false,"gen":1,"walking":false}"#), window, cx);
    });
    frame(cx, handle);
    assert_eq!(call(cx, handle, "cursor"), Reply::value("0"));
    key(cx, handle, "down");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("1"));
    key(cx, handle, "up");
    assert_eq!(call(cx, handle, "cursor"), Reply::value("0"));
    for (stroke, cursor) in [
        ("end", "1"),
        ("home", "0"),
        ("ctrl-d", "1"),
        ("ctrl-u", "0"),
    ] {
        key(cx, handle, stroke);
        assert_eq!(call(cx, handle, "cursor"), Reply::value(cursor));
    }
    key(cx, handle, "j");
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.query,
            "aj"
        );
    });
}

#[gpui::test]
fn ey172_sheet_search_lists_its_own_question_mark_binding(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "?");
    assert_eq!(call(cx, handle, "keymapSheetOpen"), Reply::value("true"));
    key(cx, handle, "?");
    assert_eq!(call(cx, handle, "keymapQuery"), Reply::value("?"));
    assert_eq!(
        call(cx, handle, "keymapSheetRows"),
        Reply::value("? Keyboard Shortcuts")
    );
}

// S9G-2: Return on the toolbar's Dual glyph keeps the keys on that glyph, as List, Columns and Grid do.
#[gpui::test]
fn s9g2_return_on_the_dual_glyph_keeps_the_glyph_focused(cx: &mut TestAppContext) {
    let handle = listing(cx);
    key(cx, handle, "tab tab");
    for _ in 0..12 {
        if call(cx, handle, "focusStop") == Reply::value("chrome:dual") {
            break;
        }
        key(cx, handle, "right");
    }
    assert_eq!(call(cx, handle, "focusStop"), Reply::value("chrome:dual"));
    key(cx, handle, "enter");
    assert_eq!(call(cx, handle, "focusStop"), Reply::value("chrome:dual"));
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.view, View::Dual, "Enter on the glyph switched")
    });
    edit(cx, handle, |shell, window, cx| {
        // The secondary's listing usually lands after the key: rewind to the pending single view and let it land.
        shell.view = View::List;
        shell.dual_pending = true;
        shell.panes[1].as_mut().expect("the secondary pane").stopped = true;
        shell.settle_dual(window, cx);
        assert!(!shell.dual_pending, "the late answer settled the switch");
        assert_eq!(shell.view, View::Dual, "the late answer switched the view");
    });
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| assert_eq!(shell.view, View::Dual));
    assert_eq!(call(cx, handle, "focusStop"), Reply::value("chrome:dual"));
}

// S11J-3: a tab strip that stops drawing while it holds the keys hands them to the list stop.
#[gpui::test]
fn s11j3_closing_the_second_tab_from_the_focused_strip_hands_the_keys_to_the_list(
    cx: &mut TestAppContext,
) {
    let handle = listing(cx);
    key(cx, handle, "t");
    key(cx, handle, "tab tab tab");
    assert_eq!(call(cx, handle, "focusView"), Reply::value("strip"));
    edit(cx, handle, |shell, window, cx| {
        shell.tab_tap_hit(crate::tabs::bar::Hit::Close(1), false, window, cx);
        // The close leaves the keys on the strip; only the next frame's stop sync may move them.
        assert!(shell.dual_focus.strip.contains_focused(window, cx));
    });
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        assert!(
            shell.slots.tabs.is_none(),
            "one tab is left, so the strip is gone"
        );
        assert!(!shell.dual_focus.strip.contains_focused(window, cx));
        assert!(
            shell.focus.is_focused(window),
            "the list stop holds the keys"
        );
    });
    assert_eq!(call(cx, handle, "focusView"), Reply::value("list"));
}
