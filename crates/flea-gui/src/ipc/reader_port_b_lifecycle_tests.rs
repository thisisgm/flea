//! Regression checks for lifecycle boundaries reported by the cold review.
use super::*;

#[test]
fn loading_interval_restarts_after_a_hidden_replacement() {
    let mut observed = crate::g2::quick_look_overlay::observations::Observations::default();
    observed.opened(0, false);
    observed.revealed(100, false, false, true);
    observed.opened(200, false);
    observed.revealed(250, false, false, true);
    observed.revealed(300, true, true, false);
    assert_eq!(observed.swap.shown_ms, 50);
}

#[gpui::test]
fn armed_identity_keeps_its_path_after_focus_and_path_change(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.view = crate::window::startup::View::Dual;
        shell.launch.settings.data = flea::jsondoc::parse(r#"{"keys":"vim"}"#).unwrap();
        shell.panes[0].as_mut().unwrap().pane.cursor = 4;
        shell.panes[1] = Some(crate::window::pane::PaneHost::new(
            "/other".into(),
            None,
            620.,
            &shell.locale,
        ));
        shell.key(
            &gpui::KeyDownEvent {
                keystroke: gpui::Keystroke {
                    key: "y".into(),
                    key_char: Some("y".into()),
                    ..Default::default()
                },
                is_held: false,
                prefer_character_input: false,
            },
            window,
            cx,
        );
        assert!(
            shell.keyboard.pending().is_some(),
            "the real key route must arm the pair"
        );
        shell.focus_pane(1, cx);
    });
    let identity = |cx: &mut TestAppContext| {
        flea::jsondoc::parse(&value(cx, handle, "keyDeliveryState"))
            .unwrap()
            .get("keySequenceIdentity")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    };
    assert_eq!(identity(cx), r#"["/files",4,0,"list"]"#);
    edit(cx, handle, |shell, _, cx| {
        shell.panes[0]
            .as_mut()
            .unwrap()
            .pane
            .receive(flea::protocol::Reply::Listed {
                path: "/new".into(),
                total: 0,
            })
            .unwrap();
        shell.focus_pane(0, cx);
    });
    assert_eq!(identity(cx), r#"["/files",4,0,"list"]"#);
}

#[gpui::test]
fn mounted_pdf_without_keyboard_focus_reports_null(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    pdf(cx, handle);
    edit(cx, handle, |_, window, cx| window.blur(cx));
    let state = flea::jsondoc::parse(&value(cx, handle, "keyDeliveryState")).unwrap();
    assert_eq!(
        state
            .get("activeFocusItem")
            .and_then(flea::jsondoc::Json::as_str),
        Some("null")
    );
}

fn press(cx: &mut TestAppContext, handle: Handle, key: &str, control: bool) {
    edit(cx, handle, |shell, window, cx| {
        shell.key(
            &gpui::KeyDownEvent {
                keystroke: gpui::Keystroke {
                    key: key.into(),
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
}

fn keyboard(cx: &mut TestAppContext, handle: Handle) -> flea::jsondoc::Json {
    flea::jsondoc::parse(&value(cx, handle, "keyDeliveryState")).unwrap()
}

#[gpui::test]
fn focus_owner_rename_blur_has_no_pane_keyboard_focus(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    press(cx, handle, "f2", false);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.rename.edit.is_some())
    });
    let focused = keyboard(cx, handle);
    assert_eq!(
        focused
            .get("paneFocus")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    assert_eq!(
        focused
            .get("listFocus")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    edit(cx, handle, |_, window, cx| window.blur(cx));
    edit(cx, handle, |shell, _, _| {
        assert!(
            shell.rename.edit.is_some(),
            "the real editor remains mounted after blur"
        )
    });
    let blurred = keyboard(cx, handle);
    assert_eq!(
        blurred
            .get("activeFocusItem")
            .and_then(flea::jsondoc::Json::as_str),
        Some("null")
    );
    assert_eq!(
        blurred
            .get("paneFocus")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
    assert_eq!(
        blurred
            .get("listFocus")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(false)
    );
}

#[gpui::test]
fn focus_owner_pdf_controls_keep_focus_until_close(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_threads(|| {
        let (handle, _read) = open(cx);
        edit(cx, handle, |shell, window, cx| {
            shell.view = crate::window::startup::View::Columns;
            let host = shell.panes[0].as_mut().unwrap();
            host.pane
                .receive(flea::protocol::Reply::Listed {
                    path: "/files".into(),
                    total: 1,
                })
                .unwrap();
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: vec![flea::protocol::RowFacts {
                        name: "book.pdf".into(),
                        icon: "application-pdf".into(),
                        mode: 0o100644,
                        ..Default::default()
                    }],
                })
                .unwrap();
            shell.sync_columns(window, cx);
            shell.sync_selection_previews(window, cx);
            shell.selection_preview(0).unwrap().update(cx, |view, _| {
                view.class = Some(("/files".into(), "local".into()))
            });
            shell.sync_selection_previews(window, cx);
        });
        press(cx, handle, "tab", true);
        assert_eq!(
            keyboard(cx, handle)
                .get("preview")
                .unwrap()
                .get("focused")
                .and_then(flea::jsondoc::Json::as_bool),
            Some(true)
        );
        edit(cx, handle, |shell, window, cx| {
            assert!(shell.open_quick_look(0, window, cx))
        });
        for _ in 0..2 {
            let state = keyboard(cx, handle);
            assert_eq!(
                state
                    .get("activeFocusItem")
                    .and_then(flea::jsondoc::Json::as_str),
                Some("PdfViewer")
            );
            assert_eq!(
                state
                    .get("preview")
                    .unwrap()
                    .get("focused")
                    .and_then(flea::jsondoc::Json::as_bool),
                Some(false)
            );
            assert_eq!(
                state
                    .get("paneFocus")
                    .and_then(flea::jsondoc::Json::as_bool),
                Some(false)
            );
            // Tab walks the PDF strip; Ctrl+Tab would close the card, as PdfViewer.qml does at v0.3.12 (EY-438).
            press(cx, handle, "tab", false);
        }
        press(cx, handle, "escape", false);
        let state = keyboard(cx, handle);
        assert_eq!(
            state
                .get("activeFocusItem")
                .and_then(flea::jsondoc::Json::as_str),
            Some("SelectionPreview")
        );
        assert_eq!(
            state
                .get("preview")
                .unwrap()
                .get("focused")
                .and_then(flea::jsondoc::Json::as_bool),
            Some(true)
        );
        assert_eq!(
            state
                .get("paneFocus")
                .and_then(flea::jsondoc::Json::as_bool),
            Some(true)
        );
    });
}

// EY-438: PdfViewer.qml at v0.3.12 keeps the cursor keys from the listing and closes on focusPreview.
#[gpui::test]
fn quick_look_pdf_takes_cursor_keys_and_closes_on_focus_preview(cx: &mut TestAppContext) {
    crate::g2::doc_preview::render::without_threads(|| {
        let (handle, _read) = open(cx);
        edit(cx, handle, |shell, window, cx| {
            shell.view = crate::window::startup::View::Columns;
            let host = shell.panes[0].as_mut().unwrap();
            host.pane
                .receive(flea::protocol::Reply::Listed {
                    path: "/files".into(),
                    total: 2,
                })
                .unwrap();
            let row = |name: &str, icon: &str| flea::protocol::RowFacts {
                name: name.into(),
                icon: icon.into(),
                mode: 0o100644,
                ..Default::default()
            };
            host.pane
                .receive(flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 3,
                    rows: vec![
                        row("book.pdf", "application-pdf"),
                        row("after.txt", "text-plain"),
                    ],
                })
                .unwrap();
            shell.sync_columns(window, cx);
            shell.sync_selection_previews(window, cx);
            shell.selection_preview(0).unwrap().update(cx, |view, _| {
                view.class = Some(("/files".into(), "local".into()))
            });
            shell.sync_selection_previews(window, cx);
            assert!(shell.open_quick_look(0, window, cx))
        });
        assert_eq!(
            keyboard(cx, handle)
                .get("activeFocusItem")
                .and_then(flea::jsondoc::Json::as_str),
            Some("PdfViewer")
        );
        press(cx, handle, "down", false);
        assert_eq!(value(cx, handle, "previewOpen"), "true");
        edit(cx, handle, |shell, _, _| {
            assert_eq!(
                shell.panes[0].as_ref().unwrap().pane.cursor,
                0,
                "Down stays in the PDF card and leaves the listing cursor"
            )
        });
        press(cx, handle, "tab", true);
        assert_eq!(
            value(cx, handle, "previewOpen"),
            "false",
            "Ctrl+Tab closes a PDF card"
        );
    });
}

#[gpui::test]
fn focus_owner_hidden_pending_rename_returns_to_listing(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    press(cx, handle, "f2", false);
    edit(cx, handle, |shell, _, _| {
        shell
            .rename
            .edit
            .as_mut()
            .unwrap()
            .entry
            .replace(None, "renamed.txt");
    });
    press(cx, handle, "enter", false);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.rename.edit.as_ref().unwrap().pending.is_some())
    });
    // S2-4: Escape cannot abandon the write, so the editor keeps the keys.
    press(cx, handle, "escape", false);
    assert_eq!(
        keyboard(cx, handle)
            .get("activeFocusItem")
            .and_then(flea::jsondoc::Json::as_str),
        Some("QQuickTextInput")
    );
    // A row that leaves the drawn listing hides the editor the way rename.rs does, with the request still out.
    edit(cx, handle, |shell, _, _| {
        let edit = shell.rename.edit.as_mut().unwrap();
        assert!(edit.entry.editing, "Escape keeps the pending editor");
        edit.entry.close();
        assert!(
            edit.pending.is_some(),
            "the reply still has its original owner"
        );
    });
    let state = keyboard(cx, handle);
    assert_eq!(
        state
            .get("activeFocusItem")
            .and_then(flea::jsondoc::Json::as_str),
        Some("List")
    );
    assert_eq!(
        state
            .get("paneFocus")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
    assert_eq!(
        state
            .get("listFocus")
            .and_then(flea::jsondoc::Json::as_bool),
        Some(true)
    );
}
