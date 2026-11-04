//! G1.12 in the window: the composer's path bar opens from the listing, asks with its favourites, ranks the answer and opens the row.
use super::*;
use crate::{
    ipc::server::Event,
    path::{view::PathView, Answer, Jumped},
    window::root::Root,
};
use gpui::{Entity, VisualTestContext, Window, WindowHandle};

type Handle = WindowHandle<Root>;

const FAVOURITES: &str =
    r#"{"places":{"rail":"hidden","favourites":[{"label":"Projects","path":"/fx/j/Projects"}]}}"#;

// The production Root over a one-pane shell whose backend records every line it is sent.
fn open(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, lines) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let mut shell = shell_with(backend, cx);
            shell.launch.settings.data = flea::jsondoc::parse(FAVOURITES).unwrap();
            shell.keyboard.context = flea::keys::Context::Listing;
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    frame(cx, handle);
    (handle, lines)
}

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
}

fn key(cx: &mut TestAppContext, handle: Handle, keys: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(keys);
    frame(cx, handle);
}

fn bar(cx: &mut TestAppContext, handle: Handle) -> Entity<PathView> {
    edit(cx, handle, |shell, _, cx| {
        let chrome = shell
            .chrome
            .view
            .clone()
            .expect("the composer mounted its chrome");
        chrome.read_with(cx, |view, _| view.path.clone())
    })
}

fn pending(cx: &mut TestAppContext, handle: Handle) -> flea::ipc::wire::Reply {
    edit(cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        let request = flea::ipc::wire::Request::Call {
            target: "flea".into(),
            function: "pendingRequests".into(),
            arguments: vec![],
        };
        shell.ipc(Event::Call { request, answer }, window, cx);
        receive.try_recv().unwrap().reply
    })
}

fn sent(lines: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    std::iter::from_fn(lines).collect()
}

fn jumped(id: u64) -> BackendEvent {
    BackendEvent::Path(Answer::Jumped(Jumped {
        id,
        favourites: vec!["/fx/j/Projects".into()],
        zoxide: vec!["/fx/j/Documents".into()],
        ..Default::default()
    }))
}

fn rows(cx: &mut TestAppContext, path: &Entity<PathView>) -> Vec<String> {
    cx.read_entity(path, |view, _| {
        view.jump
            .entries
            .iter()
            .map(|row| row.path.clone())
            .collect()
    })
}

#[gpui::test]
fn a_lost_backend_owes_no_jump_answer(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    let jumps = |cx: &mut TestAppContext| edit(cx, handle, |shell, _, _| shell.chrome.jumps);
    edit(cx, handle, |shell, window, cx| {
        shell.chrome_action("pathBar", window, cx)
    });
    cx.run_until_parked();
    assert_eq!(jumps(cx), [1, 0], "the provisional ask is out on side 0");
    let failure = crate::backend::Failure {
        key: "window.backend_eof",
        detail: String::new(),
    };
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, BackendEvent::Failed(failure), window, cx)
    });
    assert_eq!(jumps(cx), [0, 0], "a failed backend never answers its jump");
    key(cx, handle, "escape");
    edit(cx, handle, |shell, window, cx| {
        shell.chrome_action("pathBar", window, cx)
    });
    cx.run_until_parked();
    assert_eq!(jumps(cx), [1, 0], "a second ask is out on side 0");
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, BackendEvent::Drained, window, cx)
    });
    assert_eq!(
        jumps(cx),
        [0, 0],
        "a drained backend never answers its jump"
    );
    // No second pane runs here, so side 1's earlier owed ask is seeded; a fresh second pane owes none.
    edit(cx, handle, |shell, _, _| shell.chrome.jumps[1] = 1);
    // The second pane gets a recording backend, so no real backend thread wakes the test scheduler.
    cx.update(|cx| {
        cx.set_global(crate::state::constructor_fixture::Services {
            backend: |_, cx| {
                let (backend, requests) = Backend::test_channel();
                cx.on_release(move |_, _| drop(requests)).detach();
                Ok(backend)
            },
            monitor: None,
        })
    });
    edit(cx, handle, |shell, window, cx| {
        shell.request_dual(window, cx)
    });
    assert_eq!(jumps(cx)[1], 0, "request_dual starts side 1 owing no jump");
}

#[gpui::test]
fn m9_integ_path_rank_survives_live_composer_focus(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    let value = |n: usize| flea::ipc::wire::Reply::value(n.to_string());
    assert_eq!(pending(cx, handle), value(0));
    // The pathBar action's own handler, the one the listing key reaches, so the history read is still out.
    edit(cx, handle, |shell, window, cx| {
        shell.chrome_action("pathBar", window, cx)
    });
    assert_eq!(
        pending(cx, handle),
        value(2),
        "the provisional ask and the history read are in flight"
    );
    let path = bar(cx, handle);
    assert!(
        cx.read_entity(&path, |view, _| view.editing()),
        "the pathBar action opens the composer's bar"
    );
    let favourites = ["/fx/j/Projects".to_owned()];
    let asked = sent(&mut lines);
    assert!(
        asked.contains(&crate::path::jump_line(1, 0, &favourites, &[])),
        "the open reaches the pane's backend as a jump ask with the ui state's favourites: {asked:?}"
    );
    cx.run_until_parked();
    assert_eq!(
        pending(cx, handle),
        value(1),
        "the history landed; the provisional ask is still out"
    );
    edit(cx, handle, |shell, window, cx| {
        let path = shell.chrome.view.clone().unwrap().read(cx).path.clone();
        path.update(cx, |view, _| view.entry.text = "o".into());
        shell.receive(0, jumped(7), window, cx);
    });
    assert!(rows(cx, &path).is_empty(), "a stale jump id ranks nothing");
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, jumped(1), window, cx)
    });
    frame(cx, handle);
    assert_eq!(
        rows(cx, &path),
        ["/fx/j/Projects", "/fx/j/Documents"],
        "the window's ranker lists both sources, a favourite first"
    );
    assert!(
        sent(&mut lines)
            .iter()
            .any(|line| line.starts_with(r#"{"c":"jump","id":2,"ranking":1,"#)),
        "the provisional answer sends the whole ask"
    );
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, jumped(2), window, cx)
    });
    assert_eq!(
        pending(cx, handle),
        value(0),
        "every ask is answered and the scene settles"
    );
    let cursor = edit(cx, handle, |shell, window, cx| {
        assert!(
            shell.path_holds_keyboard(window, cx),
            "the ranked bar keeps the keyboard"
        );
        shell.panes[0].as_ref().unwrap().pane.cursor
    });
    key(cx, handle, "j");
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.cursor,
            cursor,
            "an open bar keeps the listing's keys"
        )
    });
    key(cx, handle, "enter");
    edit(cx, handle, |shell, window, cx| {
        assert_eq!(
            shell.panes[0].as_ref().unwrap().pane.place(),
            "/fx/j/Projects",
            "Enter opens the first ranked row"
        );
        assert!(!shell.path_holds_keyboard(window, cx), "the bar closed");
        assert!(
            shell.focus.is_focused(window),
            "the listing has the keyboard again"
        );
    });
}

#[gpui::test]
fn a_new_ui_state_replaces_the_favourites_the_next_ask_carries(cx: &mut TestAppContext) {
    let (handle, mut lines) = open(cx);
    key(cx, handle, "ctrl-l");
    key(cx, handle, "escape");
    cx.run_until_parked();
    let _ = sent(&mut lines);
    edit(cx, handle, |shell, _, cx| {
        let state = flea::jsondoc::parse(
            r#"{"places":{"rail":"hidden","favourites":[{"path":"/fx/j/Music"}]}}"#,
        )
        .unwrap();
        shell.rail_state(&state, cx);
    });
    key(cx, handle, "ctrl-l");
    let path = bar(cx, handle);
    assert!(
        cx.read_entity(&path, |view, _| view.editing()),
        "the listing's ctrl-l opens the bar"
    );
    let asked = sent(&mut lines);
    assert!(
        asked.iter().any(|line| line.starts_with(r#"{"c":"jump""#)
            && line.contains(r#""favourites":["/fx/j/Music"]"#)),
        "the reopened bar asks with the accepted state's favourites: {asked:?}"
    );
}

#[test]
fn the_history_worker_reads_the_recent_file_and_stamps_it() {
    let parent =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.superpowers/g1-12");
    std::fs::create_dir_all(&parent).unwrap();
    let made = std::process::Command::new("mktemp")
        .arg("-d")
        .arg(parent.join("history.XXXXXXXX"))
        .output()
        .unwrap();
    let root = std::path::PathBuf::from(String::from_utf8(made.stdout).unwrap().trim());
    assert!(made.status.success() && root.is_absolute());
    std::fs::write(root.join(".flea-test-sandbox"), "g1-12\n").unwrap();
    let file = root.join("recently-used.xbel");
    assert_eq!(
        crate::window::path_host::load(&file),
        (None, Vec::new()),
        "an absent history is no history"
    );
    std::fs::write(
        &file,
        r#"<xbel><bookmark href="file:///srv/a.txt" visited="2026-01-01T00:00:00Z"/></xbel>"#,
    )
    .unwrap();
    let (stamp, paths) = crate::window::path_host::load(&file);
    assert_eq!(paths, ["/srv/a.txt"]);
    assert_eq!(
        stamp.map(|(_, len)| len),
        Some(std::fs::metadata(&file).unwrap().len())
    );
    let unreadable = root.join("unreadable.xbel");
    std::fs::create_dir(&unreadable).unwrap();
    assert_eq!(
        crate::window::path_host::load(&unreadable),
        (None, Vec::new()),
        "a failed read keeps no stamp, so the next open reads again"
    );
    assert!(root.starts_with(&parent) && root.join(".flea-test-sandbox").is_file());
    std::fs::remove_dir_all(&root).unwrap();
}
