//! G1.12 in the window: the listing's keys open the bar, an open bar keeps them, and its answers reach the panes.
use super::*;
use crate::path::view::Out;
use gpui::{KeyDownEvent, Keystroke, Modifiers};

fn key(key: &str, text: Option<&str>, modifiers: Modifiers) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            key: key.into(),
            key_char: text.map(str::to_owned),
            modifiers,
            is_keypad: false,
        },
        is_held: false,
        prefer_character_input: false,
    }
}

fn call(
    shell: &mut Shell,
    function: &str,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> flea::ipc::wire::Reply {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    let request = flea::ipc::wire::Request::Call {
        target: "flea".into(),
        function: function.into(),
        arguments: vec![],
    };
    shell.ipc(Event::Call { request, answer }, window, cx);
    receive.try_recv().unwrap().reply
}

#[gpui::test]
fn the_listing_keys_open_the_bar_and_an_open_bar_keeps_the_listing_keys(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let shell = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.keyboard.context = flea::keys::Context::Editor;
            shell.key(&key(":", Some(":"), Modifiers::shift()), window, cx);
            assert!(
                !shell.path.read(cx).editing(),
                "the bar opens from the listing only"
            );
            shell.keyboard.context = flea::keys::Context::Listing;
            shell.key(&key(":", Some(":"), Modifiers::shift()), window, cx);
            assert!(
                shell.path.read(cx).editing(),
                "tests/js/pathbar.js:238, the colon asks the shell to open the bar"
            );
            assert_eq!(shell.path.read(cx).entry.text, "/files/");
            assert_eq!(
                call(shell, "pathBarOpen", window, cx),
                flea::ipc::wire::Reply::value("true")
            );
            assert_eq!(
                call(shell, "pathBarText", window, cx),
                flea::ipc::wire::Reply::value("/files/")
            );
            let cursor = shell.panes[0].as_ref().unwrap().pane.cursor;
            shell.key(&key("j", Some("j"), Modifiers::none()), window, cx);
            shell.key(&key("down", None, Modifiers::none()), window, cx);
            assert_eq!(
                shell.panes[0].as_ref().unwrap().pane.cursor,
                cursor,
                "an open bar keeps the listing's keys"
            );
            shell.path.update(cx, |view, cx| view.close(cx));
            assert_eq!(
                call(shell, "pathBarOpen", window, cx),
                flea::ipc::wire::Reply::value("false")
            );
            shell.key(&key("l", None, Modifiers::control()), window, cx);
            assert!(shell.path.read(cx).editing(), "ctrl-l opens it as well");
        })
    });
}

#[gpui::test]
fn a_failed_listing_retries_and_each_answer_reaches_its_pane(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let shell = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            let path = shell.path.clone();
            assert!(!shell.path_inputs().failed);
            shell.panes[0].as_mut().unwrap().pane.state.kind = crate::pane::Kind::Error;
            let host = shell.panes[0].as_ref().unwrap();
            assert!(
                shell.path_inputs().failed,
                "tests/js/pathbar.js:144, the failed listing is the retry fact"
            );
            assert_eq!(
                shell.path_inputs().failed,
                host.nav.path_failed(&host.pane),
                "the bar and the navigator agree"
            );
            shell.open_path_bar(window, cx);
            assert!(
                path.read(cx).focus.contains_focused(window, cx),
                "the open field holds the keyboard"
            );
            shell.path_out(
                &path,
                &Out::Open {
                    side: 0,
                    path: "/files/x".into(),
                },
                window,
                cx,
            );
            let host = shell.panes[0].as_ref().unwrap();
            assert_eq!(
                (host.pane.place(), host.nav.counters.lists),
                ("/files/x", 1)
            );
            shell.path_out(
                &path,
                &Out::Said("Nothing in /files starts with that".into()),
                window,
                cx,
            );
            assert_eq!(
                shell.panes[0].as_ref().unwrap().status.as_ref(),
                "Nothing in /files starts with that"
            );
            shell.path_out(&path, &Out::Network("smb://server.lan".into()), window, cx);
            let unavailable = crate::path::Said::NetworkUnavailable.text(&shell.locale.catalog);
            assert_eq!(
                shell.panes[0].as_ref().unwrap().status.as_ref(),
                unavailable
            );
            shell.path_out(
                &path,
                &Out::Send {
                    side: 0,
                    line: crate::path::peek_line("/", 40, false),
                },
                window,
                cx,
            );
            assert!(
                shell.panes[0].as_ref().unwrap().error.is_none(),
                "the peek reached a live backend"
            );
            shell.path_out(&path, &Out::Focus(1), window, cx);
            assert_eq!(
                shell.focus_side, 0,
                "a single view has no second pane to focus"
            );
            path.update(cx, |view, cx| view.close(cx));
            shell.path_out(&path, &Out::Closed, window, cx);
            assert!(
                shell.focus.is_focused(window),
                "the closed bar hands the keyboard back to the listing"
            );
        })
    });
}
