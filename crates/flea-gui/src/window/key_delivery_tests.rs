//! keyDeliveryState answers v0.3.12's whole shape from each live owner, through the production IPC dispatcher and real keys.
use crate::{
    backend::{Backend, Event as BackendEvent},
    ipc::server::Event,
    navigation::history::Entry,
    state::{tests::shell_with, Shell},
    window::root::Root,
};
use flea::{
    ipc::wire::Request,
    jsondoc::{self, Json},
};
use gpui::{AppContext, Context, TestAppContext, VisualTestContext, Window, WindowHandle};

type Handle = WindowHandle<Root>;

// The caller holds the read end for the test, so a request the owners send never fails and ends their session.
fn open(cx: &mut TestAppContext) -> (Handle, Box<dyn FnMut() -> Option<String>>) {
    let (backend, lines) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    edit(cx, handle, |shell, window, cx| {
        let pane = &mut shell.panes[0].as_mut().unwrap().pane;
        assert!(pane.begin("/files".into(), Default::default(), pane.now()));
        for reply in [
            flea::protocol::Reply::Listed {
                path: "/files".into(),
                total: 3,
            },
            flea::protocol::Reply::Rows {
                start: 0,
                listing: 3,
                rows: ["a.txt", "b.txt", "c.txt"]
                    .iter()
                    .map(|name| flea::protocol::RowFacts {
                        name: (*name).into(),
                        ..Default::default()
                    })
                    .collect(),
            },
        ] {
            shell.receive(0, BackendEvent::Reply(reply), window, cx);
        }
        shell.panes[0].as_mut().unwrap().pane.cursor = 0;
    });
    frame(cx, handle);
    (handle, Box::new(lines))
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

fn key(cx: &mut TestAppContext, handle: Handle, value: &str) {
    VisualTestContext::from_window(handle.into(), cx).simulate_keystrokes(value);
    frame(cx, handle);
}

// Drives the production Search owner the way the search key and the query line do.
fn search(
    cx: &mut TestAppContext,
    handle: Handle,
    change: impl FnOnce(&mut crate::g2::file_search::Search, &mut dyn crate::g2::file_search::Host),
) {
    edit(cx, handle, |shell, window, cx| {
        shell.search_update(0, None, change, window, cx)
    });
    frame(cx, handle);
}

fn reply(cx: &mut TestAppContext, handle: Handle, line: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(line), window, cx)
    });
    frame(cx, handle);
}

fn state(cx: &mut TestAppContext, handle: Handle) -> Json {
    let (answer, receive) = std::sync::mpsc::sync_channel(1);
    edit(cx, handle, |shell, window, cx| {
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: "keyDeliveryState".into(),
                    arguments: Vec::new(),
                },
                answer,
            },
            window,
            cx,
        );
    });
    let answer = receive
        .try_recv()
        .expect("the mounted reader answers synchronously");
    assert_eq!(answer.reply.code, 0, "{}", answer.reply.stderr);
    assert!(
        !answer.reply.stdout.trim_end().contains('\n'),
        "one line, as JSON.stringify printed it: {}",
        answer.reply.stdout
    );
    jsondoc::parse(&answer.reply.stdout).expect("keyDeliveryState is JSON")
}

fn field<'a>(json: &'a Json, key: &str) -> &'a Json {
    json.get(key)
        .unwrap_or_else(|| panic!("keyDeliveryState has no {key}"))
}

fn parse(text: &str) -> Json {
    jsondoc::parse(text).unwrap()
}

#[gpui::test]
fn key_delivery_state_carries_every_v0312_key_in_order(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    let read = state(cx, handle);
    let Json::Obj(pairs) = &read else {
        panic!("an object: {read:?}")
    };
    let keys: Vec<&str> = pairs.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "keySequence",
            "keySequenceIdentity",
            "trashArmedAt",
            "clipboard",
            "searchMode",
            "filterTyping",
            "filterQuery",
            "searchRunning",
            "searchQuery",
            "searchScanned",
            "searchCancelled",
            "paneFocus",
            "listFocus",
            "activeFocusItem",
            "preview",
            "history",
        ]
    );
    assert_eq!(
        read,
        parse(
            r#"{"keySequence":"","keySequenceIdentity":"","trashArmedAt":0,"clipboard":{"paths":[],"cut":false},
            "searchMode":"","filterTyping":false,"filterQuery":"","searchRunning":false,"searchQuery":"",
            "searchScanned":0,"searchCancelled":false,"paneFocus":true,"listFocus":true,"activeFocusItem":"List",
            "preview":{"index":-1,"focused":false},"history":{"back":[],"forward":[]}}"#
        ),
        "a browsing window at rest"
    );
}

#[gpui::test]
fn key_delivery_state_reads_the_vim_pair_arm(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    edit(cx, handle, |shell, _, _| {
        let Json::Obj(pairs) = &mut shell.launch.settings.data else {
            panic!("settings are an object")
        };
        pairs.retain(|(key, _)| key != "keys");
        pairs.push(("keys".into(), Json::Str("vim".into())));
    });
    key(cx, handle, "g");
    let armed = state(cx, handle);
    assert_eq!(
        field(&armed, "keySequence"),
        &Json::Str("cursorFirstArm".into())
    );
    assert!(
        field(&armed, "keySequenceIdentity")
            .as_str()
            .is_some_and(|identity| !identity.is_empty()),
        "the arm names its seat: {armed:?}"
    );
    key(cx, handle, "g");
    let fired = state(cx, handle);
    assert_eq!(field(&fired, "keySequence"), &Json::Str(String::new()));
    assert_eq!(
        field(&fired, "keySequenceIdentity"),
        &Json::Str(String::new())
    );
}

#[gpui::test]
fn key_delivery_state_reads_the_dd_arm_and_escape_ends_it(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    key(cx, handle, "d");
    let armed = state(cx, handle);
    assert!(
        field(&armed, "trashArmedAt")
            .as_f64()
            .is_some_and(|stamp| stamp > 0.),
        "the first d stamps the arm: {armed:?}"
    );
    key(cx, handle, "escape");
    assert_eq!(
        field(&state(cx, handle), "trashArmedAt"),
        &Json::Num("0".into()),
        "Escape ends the arm"
    );
}

#[gpui::test]
fn key_delivery_state_reads_the_window_clipboard(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    key(cx, handle, "ctrl-x");
    reply(cx, handle, r#"{"t":"paths","paths":["/files/a.txt"]}"#);
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"set","ok":true,"token":"T1"}"#,
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"cut","paths":["/files/a.txt"],"token":"T1","skipped":0}"#,
    );
    assert_eq!(
        field(&state(cx, handle), "clipboard"),
        &parse(r#"{"paths":["/files/a.txt"],"cut":true}"#)
    );
    reply(
        cx,
        handle,
        r#"{"t":"clip","op":"changed","clip":"copy","paths":["/o/x \"y\".txt","/o/z.txt"],"token":"T2"}"#,
    );
    assert_eq!(
        field(&state(cx, handle), "clipboard"),
        &parse(r#"{"paths":["/o/x \"y\".txt","/o/z.txt"],"cut":false}"#)
    );
}

#[gpui::test]
fn key_delivery_state_reads_search_and_its_focus(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    search(cx, handle, |search, host| {
        assert!(search.action("search", host))
    });
    let typing = state(cx, handle);
    assert_eq!(field(&typing, "searchMode"), &Json::Str("typing".into()));
    assert_eq!(field(&typing, "searchQuery"), &Json::Str(String::new()));
    search(cx, handle, |search, host| {
        search.key(crate::g2::file_search::StripKey::Text("n".into()), host)
    });
    let typed = state(cx, handle);
    assert_eq!(field(&typed, "searchQuery"), &Json::Str("n".into()));
    assert_eq!(field(&typed, "filterTyping"), &Json::Bool(false));
    assert_eq!(
        field(&typed, "paneFocus"),
        &Json::Bool(true),
        "the query line is inside the pane"
    );
    assert_eq!(
        field(&typed, "listFocus"),
        &Json::Bool(false),
        "the list does not hold the caret"
    );
    assert_eq!(
        field(&typed, "activeFocusItem"),
        &Json::Str("QQuickTextInput".into())
    );
    edit(cx, handle, |shell, _, _| {
        let session = &mut shell.panes[0].as_mut().unwrap().search.owner.session;
        session.running = true;
        session.scanned = 100010;
        session.cancelled = true;
        session.mode = flea::ui_logic_file_search::Mode::Results;
    });
    let results = state(cx, handle);
    assert_eq!(field(&results, "searchMode"), &Json::Str("results".into()));
    assert_eq!(field(&results, "searchRunning"), &Json::Bool(true));
    assert_eq!(
        field(&results, "searchScanned"),
        &Json::Num("100010".into())
    );
    assert_eq!(field(&results, "searchCancelled"), &Json::Bool(true));
}

#[gpui::test]
fn key_delivery_state_reads_the_filter_line(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    key(cx, handle, "/");
    let open_line = state(cx, handle);
    assert_eq!(field(&open_line, "filterTyping"), &Json::Bool(true));
    assert_eq!(field(&open_line, "filterQuery"), &Json::Str(String::new()));
    assert_eq!(field(&open_line, "listFocus"), &Json::Bool(false));
    assert_eq!(field(&open_line, "paneFocus"), &Json::Bool(true));
    key(cx, handle, "a");
    assert_eq!(
        field(&state(cx, handle), "filterQuery"),
        &Json::Str("a".into())
    );
    key(cx, handle, "enter");
    let committed = state(cx, handle);
    assert_eq!(field(&committed, "filterTyping"), &Json::Bool(false));
    assert_eq!(field(&committed, "filterQuery"), &Json::Str("a".into()));
    assert_eq!(field(&committed, "listFocus"), &Json::Bool(true));
}

#[gpui::test]
fn key_delivery_state_reads_history_and_preview(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    edit(cx, handle, |shell, _, _| {
        let history = &mut shell.panes[0].as_mut().unwrap().nav.history;
        history.back = vec![Entry::new("/a", "x", 0), Entry::new("/a/b", "y", 1)];
        history.forward = vec![Entry::new("/c", "z", 2)];
    });
    let read = state(cx, handle);
    assert_eq!(
        field(&read, "history"),
        &parse(r#"{"back":["/a","/a/b"],"forward":["/c"]}"#)
    );
    assert_eq!(
        field(&read, "preview"),
        &parse(r#"{"index":-1,"focused":false}"#),
        "no column preview drawn"
    );
}

#[gpui::test]
fn key_delivery_state_counts_chrome_and_strip_outside_the_pane(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    for (stop, name) in [(0, "chrome"), (1, "strip")] {
        edit(cx, handle, |shell, window, cx| {
            let target = if stop == 0 {
                &shell.dual_focus.chrome
            } else {
                &shell.dual_focus.strip
            };
            window.focus(target, cx);
        });
        frame(cx, handle);
        let read = state(cx, handle);
        assert_eq!(
            field(&read, "activeFocusItem"),
            &Json::Str("QQuickItem".into())
        );
        assert_eq!(
            edit(cx, handle, |shell, window, cx| shell.focus_view(window, cx)),
            name
        );
        assert_eq!(
            field(&read, "paneFocus"),
            &Json::Bool(false),
            "{name} belongs to the window, not the pane"
        );
        assert_eq!(field(&read, "listFocus"), &Json::Bool(false));
    }
}

#[gpui::test]
fn key_delivery_state_reads_no_focus_under_a_modal(cx: &mut TestAppContext) {
    let (handle, _lines) = open(cx);
    assert_eq!(field(&state(cx, handle), "listFocus"), &Json::Bool(true));
    edit(cx, handle, |shell, window, _| {
        assert!(shell.focus.is_focused(window));
        shell.input.blocked = true;
    });
    let read = state(cx, handle);
    assert_eq!(
        field(&read, "activeFocusItem"),
        &Json::Str("List".into()),
        "the diagnostic still describes the actual focus item; the block only changes semantic ownership"
    );
    assert_eq!(field(&read, "paneFocus"), &Json::Bool(false));
    assert_eq!(field(&read, "listFocus"), &Json::Bool(false));
}
