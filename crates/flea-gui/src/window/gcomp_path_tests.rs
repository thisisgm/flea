//! The path claim and the mounted Columns consumer share one complete backend peek.
use super::*;
use crate::columns::peeks::{Lens, Peeked};

#[gpui::test]
fn uish_misc_mounted_address_field_commits_batched_text_and_enter(cx: &mut TestAppContext) {
    let handle = open(cx);
    let stroke = |cx: &mut TestAppContext, text: &str| {
        // Sample keystrokes: "ctrl-l", "/", "D", "enter".
        cx.dispatch_keystroke(handle.into(), gpui::Keystroke::parse(text).unwrap());
    };
    stroke(cx, "ctrl-l");
    frame(cx, handle);
    let path = edit(cx, handle, |shell, _, cx| {
        shell.chrome.view.as_ref().unwrap().read(cx).path.clone()
    });
    let target = "/files/Dropbox";
    for character in target.chars() {
        stroke(cx, &character.to_string());
    }
    edit(cx, handle, |_, window, cx| {
        let view = path.read(cx);
        assert!(view.holds_keyboard(window));
        assert_eq!(view.entry.text, target);
    });
    stroke(cx, "enter");
    frame(cx, handle);
    edit(cx, handle, |shell, window, cx| {
        assert!(!path.read(cx).entry.editing);
        assert_eq!(path.read(cx).entry.text, target);
        assert_eq!(shell.focus_view(window, cx), "list");
    });
}

#[test]
fn gcomp_batch6_foreign_peek_reaches_the_real_columns_owner_once_without_losing_wire_fields() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests) = super::batch::columns(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        crate::window::columns_tests::host::settle(shell, window, cx);
    });
    frame(&mut cx, handle);
    let path = "/srv/a/b";
    let document = edit(&mut cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_mut().unwrap();
        let folders = host.nav.folders.borrow();
        let orders = |path: &str| folders.peek_order(path);
        let lens = Lens {
            here: host.pane.path(),
            hidden: folders.hidden,
            hidden_last: folders.hidden_last,
            first: host.pane.first_count(),
            orders: &orders,
        };
        let columns = host.columns.as_mut().unwrap();
        columns.area.ask(&lens, path, true, true);
        columns.area.ask(&lens, path, true, true);
        drop(folders);
        crate::window::columns_tests::host::document(
            shell,
            path,
            &[("foreign-dir", true), ("foreign-file", false)],
            false,
        )
    });
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        let mut document = document;
        let Json::Obj(fields) = &mut document else {
            unreachable!()
        };
        fields.extend([
            ("v".into(), Json::Num("917".into())),
            ("w".into(), Json::Bool(false)),
            ("gcompUnchanged".into(), Json::Str("complete wire".into())),
        ]);
        let Json::Arr(rows) = &mut fields.iter_mut().find(|(key, _)| key == "rows").unwrap().1
        else {
            unreachable!()
        };
        let Json::Obj(file) = &mut rows[1] else {
            unreachable!()
        };
        file.push(("i".into(), Json::Str("application-special".into())));
        let parsed = Peeked::from_json(&document).unwrap().unwrap();
        let sent = parsed.sent();
        let key = parsed.key();
        let before = {
            let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
            assert!(
                columns.area.outbox.is_empty(),
                "real frame flushed actual owned asks"
            );
            (
                columns.area.peeks.pending.count(&sent),
                columns.area.peeks.version,
            )
        };
        assert!(before.0 >= 2, "both producer asks are outstanding");
        let chrome = shell.chrome.view.as_ref().unwrap();
        let path_view = chrome.read_with(cx, |chrome, _| chrome.path.clone());
        assert!(!path_view.read_with(cx, |path, _| path
            .claims(&crate::path::answer(&document).unwrap())));
        let event = Backend::test_reply_line(&flea::jsondoc::render(&document));
        assert!(
            matches!(&event, BackendEvent::Peeked(kept) if kept == &document),
            "production parser retained every field"
        );
        shell.receive(0, event, window, cx);
        let columns = shell.panes[0].as_ref().unwrap().columns.as_ref().unwrap();
        assert_eq!(
            columns.area.peeks.pending.count(&sent),
            before.0 - 1,
            "one reply settles exactly one ask"
        );
        assert_eq!(
            columns.area.peeks.version,
            before.1 + 1,
            "actual U09 consumer lands exactly once"
        );
        let kept = columns.area.peeks.rows(&key).unwrap();
        assert_eq!(&**kept, &*parsed.rows);
        assert_eq!(kept[1].name, "foreign-file");
        assert_eq!(kept[1].icon, "application-special");
        let facts = columns.area.peeks.source(path).unwrap();
        assert_eq!(facts.dev, 917);
        assert!(!facts.writable);
    });
    frame(&mut cx, handle);
}

#[test]
fn gcomp_batch6_path_owned_peek_stays_with_path_and_never_reaches_columns() {
    let mut cx = crate::list::bench::real_cx();
    let (handle, _requests) = super::batch::columns(&mut cx);
    frame(&mut cx, handle);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.chrome_action("pathBar", window, cx);
        let path = shell
            .chrome
            .view
            .as_ref()
            .unwrap()
            .read_with(cx, |chrome, _| chrome.path.clone());
        path.update(cx, |path, _| {
            path.entry.text = "/srv/a/b/".into();
            assert!(matches!(
                path.entry.complete("/srv/a/b/c", "", false),
                crate::path::entry::Tab::Asked(_)
            ));
        });
        let document = crate::window::columns_tests::host::document(
            shell,
            "/srv/a/b",
            &[("owned-dir", true)],
            false,
        );
        let before = shell.panes[0]
            .as_ref()
            .unwrap()
            .columns
            .as_ref()
            .unwrap()
            .area
            .peeks
            .version;
        assert!(path.read_with(cx, |path, _| path
            .claims(&crate::path::answer(&document).unwrap())));
        shell.receive(
            0,
            Backend::test_reply_line(&flea::jsondoc::render(&document)),
            window,
            cx,
        );
        assert!(path.read_with(cx, |path, _| path.entry.pending().is_none()));
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .columns
                .as_ref()
                .unwrap()
                .area
                .peeks
                .version,
            before,
            "path-owned completion must not be offered to Columns too"
        );
    });
}

#[test]
fn gcomp_batch6_toolbar_uses_the_real_u11_settings_and_folder_view_owner() {
    use gpui::{point, px, Modifiers, MouseButton, VisualTestContext};
    let mut cx = crate::list::bench::real_cx();
    let handle = open(&mut cx);
    edit(&mut cx, handle, |shell, window, cx| {
        shell.set_view(View::List, window, cx);
        shell.launch.settings.data = flea::uistate::from_file(
            r#"{"view":"list","rememberViewPerFolder":true,"folderViews":{"/other":"columns"}}"#,
        );
    });
    frame(&mut cx, handle);
    for glyph in ["columns", "sliders"] {
        let at = edit(&mut cx, handle, |shell, window, cx| {
            let (answer, receive) = std::sync::mpsc::sync_channel(1);
            shell.ipc(
                Event::Call {
                    request: Request::Call {
                        target: "flea".into(),
                        function: "chromeButtonCentre".into(),
                        arguments: vec![glyph.into()],
                    },
                    answer,
                },
                window,
                cx,
            );
            let reply = receive.try_recv().unwrap().reply;
            assert_eq!(reply.code, 0);
            let values: Vec<f32> = reply
                .stdout
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            assert_eq!(values.len(), 2, "real mounted control centre");
            point(px(values[0]), px(values[1]))
        });
        {
            let mut pointer = VisualTestContext::from_window(handle.into(), &cx);
            pointer.simulate_mouse_move(at, None, Modifiers::none());
            pointer.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
            pointer.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        }
        frame(&mut cx, handle);
        edit(&mut cx, handle, |shell, window, cx| {
            if glyph == "columns" {
                assert_eq!(shell.view, View::Columns);
                let data = &shell.launch.settings.data;
                assert_eq!(
                    data.get("view").and_then(Json::as_str),
                    Some("list"),
                    "pane choice keeps global default"
                );
                assert_eq!(
                    flea::ui_logic_settings_pages::pages::at(data, &["folderViews", "/files"])
                        .and_then(Json::as_str),
                    Some("columns")
                );
                assert_eq!(
                    flea::ui_logic_settings_pages::pages::at(data, &["folderViews", "/other"])
                        .and_then(Json::as_str),
                    Some("columns")
                );
            } else {
                assert!(
                    shell.settings.opened,
                    "actual U11 Settings producer must open"
                );
                let panel = shell.settings.panel.as_ref().unwrap();
                assert!(
                    panel.read_with(cx, |view, _| view.focus.is_focused(window)),
                    "Settings keeps its own keyboard"
                );
                assert!(!shell.focus.is_focused(window));
            }
        });
    }
}
