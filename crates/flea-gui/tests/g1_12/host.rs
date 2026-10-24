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

#[gpui::test]
fn the_window_bar_asks_with_its_favourites_and_ranks_the_answer(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let cx = cx.add_empty_window();
    let shell = cx.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    let state = flea::jsondoc::parse(r#"{"places":{"favourites":[{"label":"Projects","path":"/fx/j/Projects"}]}}"#).unwrap();
    let path = cx.update(|_, cx| crate::window::path_host::path_surface(Arc::new(flea::i18n::I18n::default()), &state, cx));
    let sent = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = sent.clone();
    let _events = cx.update(|_, cx| cx.subscribe(&path, move |_, out: &Out, _| log.borrow_mut().push(out.clone())));
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            shell.path = path.clone();
            shell.open_path_bar(window, cx);
        })
    });
    let favourites = ["/fx/j/Projects".to_owned()];
    let ask = Out::Send { side: 0, line: crate::path::jump_line(1, 0, &favourites, &[]) };
    assert_eq!(sent.borrow().first(), Some(&ask), "an open asks the backend for the jump with the ui state's favourites");
    assert!(sent.borrow().contains(&Out::ReadHistory));
    let answer = crate::path::Jumped { id: 1, favourites: favourites.to_vec(), zoxide: vec!["/fx/j/Documents".into()], ..Default::default() };
    cx.update(|window, cx| {
        path.update(cx, |view, cx| {
            view.entry.text = "o".into();
            view.receive(crate::path::Answer::Jumped(answer), window, cx);
        })
    });
    let rows: Vec<String> = cx.update(|_, cx| path.read(cx).jump.entries.iter().map(|row| row.path.clone()).collect());
    assert_eq!(rows, ["/fx/j/Projects", "/fx/j/Documents"], "the window's ranker lists both sources, a favourite first on a tie");
}

#[test]
fn the_history_worker_reads_the_recent_file_and_stamps_it() {
    let parent = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-12");
    std::fs::create_dir_all(&parent).unwrap();
    let made = std::process::Command::new("mktemp").arg("-d").arg(parent.join("history.XXXXXXXX")).output().unwrap();
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(made.status.success() && root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g1-12\n").unwrap();
    let file = root.join("recently-used.xbel");
    assert_eq!(crate::window::path_host::load(&file), (None, Vec::new()), "an absent history is no history");
    std::fs::write(&file, r#"<xbel><bookmark href="file:///srv/a.txt" visited="2026-01-01T00:00:00Z"/></xbel>"#).unwrap();
    let (stamp, paths) = crate::window::path_host::load(&file);
    assert_eq!(paths, ["/srv/a.txt"]);
    assert_eq!(stamp.map(|(_, len)| len), Some(std::fs::metadata(&file).unwrap().len()));
}

#[gpui::test]
fn a_settled_window_waits_for_the_jump_and_its_history_until_each_lands(cx: &mut TestAppContext) {
    let (backend, _requests) = Backend::test_channel();
    let window = cx.add_empty_window();
    let shell = window.update(|_, cx| cx.new(|cx| shell_with(backend, cx)));
    let path = window.update(|_, cx| crate::window::path_host::path_surface(Arc::new(flea::i18n::I18n::default()), &flea::jsondoc::Json::Null, cx));
    let outs = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = outs.clone();
    let _events = window.update(|_, cx| cx.subscribe(&path, move |_, out: &Out, _| log.borrow_mut().push(out.clone())));
    let repaints = Rc::new(std::cell::Cell::new(0));
    let count = repaints.clone();
    let _repaints = window.update(|_, cx| cx.observe(&path, move |_, _| count.set(count.get() + 1)));
    // Each step routes what the bar emitted, as the window's own subscription does, then reads the scene's settled fact.
    let step = |window: &mut gpui::VisualTestContext, act: &dyn Fn(&mut Shell, &mut Window, &mut Context<Shell>)| {
        window.update(|window, cx| shell.update(cx, |shell, cx| act(shell, window, cx)));
        window.update(|window, cx| {
            shell.update(cx, |shell, cx| {
                for out in outs.take() {
                    shell.path_out(&path, &out, window, cx);
                }
                call(shell, "pendingRequests", window, cx)
            })
        })
    };
    let pending = |n: usize| flea::ipc::wire::Reply::value(n.to_string());
    let jumped = |id| BackendEvent::Path(crate::path::Answer::Jumped(crate::path::Jumped { id, ..Default::default() }));
    assert_eq!(step(window, &|shell, _, _| shell.path = path.clone()), pending(0));
    let opened = step(window, &|shell, window, cx| shell.key(&key("l", None, Modifiers::control()), window, cx));
    assert_eq!(opened, pending(2), "the open's provisional ask and its history read are in flight, so the scene is not settled");
    window.run_until_parked();
    assert_eq!(step(window, &|_, _, _| {}), pending(1), "the history landed; the provisional ask is still out");
    let before = repaints.get();
    assert_eq!(step(window, &|shell, window, cx| shell.receive(0, jumped(1), window, cx)), pending(1), "the provisional answer sends the whole ask");
    assert!(repaints.get() > before, "a landing answer draws a frame, so the scene must count it until then");
    assert_eq!(step(window, &|shell, window, cx| shell.receive(0, jumped(2), window, cx)), pending(0), "the whole answer lands and nothing is left");
    let settled = repaints.get();
    window.run_until_parked();
    assert_eq!(repaints.get(), settled, "a settled bar draws no later frame");
}
