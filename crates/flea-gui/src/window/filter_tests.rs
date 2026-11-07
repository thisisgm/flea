//! Filter040 at the real consumer: Shell keys, the backend request, the `filtered` answer through the window's transport, and the rows List and Grid draw.
use crate::{
    backend::Backend,
    state::{tests::shell_with, Shell},
    window::{root::Root, startup::View},
};
use gpui::{
    AppContext, Context, KeyDownEvent, Keystroke, Modifiers, TestAppContext, Window, WindowHandle,
};

type Handle = WindowHandle<Root>;

fn open(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (backend, read) = Backend::test_lines();
    let handle = cx.add_window(|window, cx| Root {
        shell: cx.new(|cx| {
            let shell = shell_with(backend, cx);
            window.focus(&shell.focus, cx);
            shell
        }),
    });
    (handle, read)
}

fn edit<R>(
    cx: &mut TestAppContext,
    handle: Handle,
    f: impl FnOnce(&mut Shell, &mut Window, &mut Context<Shell>) -> R,
) -> R {
    handle
        .update(cx, |root, window, cx| {
            root.shell.update(cx, |shell, cx| f(shell, window, cx))
        })
        .unwrap()
}

fn press(cx: &mut TestAppContext, handle: Handle, key: &str, text: Option<&str>, control: bool) {
    edit(cx, handle, |shell, window, cx| {
        let keystroke = Keystroke {
            key: key.into(),
            key_char: text.map(Into::into),
            modifiers: Modifiers {
                control,
                ..Default::default()
            },
            ..Default::default()
        };
        shell.key(
            &KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            },
            window,
            cx,
        )
    });
}

fn reply(cx: &mut TestAppContext, handle: Handle, line: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(0, Backend::test_reply_line(line), window, cx)
    });
}

fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn shown(cx: &mut TestAppContext, handle: Handle) -> Option<Vec<usize>> {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .filter
            .shown
            .as_deref()
            .map(<[usize]>::to_vec)
    })
}

fn drawn_list(cx: &mut TestAppContext, handle: Handle) -> Vec<(usize, usize)> {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .visible()
            .iter()
            .map(|row| (row.view, row.index))
            .collect()
    })
}

#[gpui::test]
fn m9_integ_filter_protocol_drives_all_view_rows(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    while read().is_some() {}
    // "/" gives the line the caret and mounts the strip; nothing is asked until a key is typed.
    press(cx, handle, "/", Some("/"), false);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().filter.owner.typing());
        assert!(
            shell.slots.strips[0].is_some(),
            "the strip slot holds the filter line"
        );
    });
    assert_eq!(read(), None);
    press(cx, handle, "n", Some("n"), false);
    assert_eq!(
        read().as_deref(),
        Some(r#"{"c":"filter","query":"n","id":1}"#),
        "the typed key asks the backend for the whole folder"
    );
    // A reply to no request in force never lands, nor one for another numbering.
    reply(
        cx,
        handle,
        r#"{"t":"filtered","id":7,"query":"n","n":1000,"rows":[1],"ms":0.010,"listing":2}"#,
    );
    reply(
        cx,
        handle,
        r#"{"t":"filtered","id":1,"query":"n","n":1000,"rows":[1],"ms":0.010,"listing":9}"#,
    );
    assert_eq!(
        shown(cx, handle),
        None,
        "a stale id or numbering is refused before a row is read"
    );
    reply(
        cx,
        handle,
        r#"{"t":"filtered","id":1,"query":"n","n":1000,"rows":[3,5,700],"ms":0.010,"listing":2}"#,
    );
    assert_eq!(shown(cx, handle), Some(vec![3, 5, 700]));
    frame(cx, handle);
    assert_eq!(
        drawn_list(cx, handle),
        vec![(0, 3), (1, 5), (2, 700)],
        "List draws the drawn rows at view positions"
    );
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.pane.cursor, 3,
            "a hidden cursor takes the first row standing"
        );
        let owed = host
            .pane
            .requests
            .outstanding
            .expect("an unheld drawn row asks for its window");
        assert!(
            owed.start <= 700 && owed.start + owed.count > 700,
            "{owed:?}"
        );
        let text = host.filter.strip.borrow().text.clone();
        assert_eq!(
            (text.query.as_ref(), text.caret, text.summary.as_ref()),
            ("n", true, "3 of 1,000 shown")
        );
        assert_eq!(host.status.as_ref(), "3 of 1,000 shown");
    });
    edit(cx, handle, |shell, _, _| shell.view = View::Grid);
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let grid = shell.panes[0]
            .as_ref()
            .unwrap()
            .grid
            .as_ref()
            .expect("Grid mounted");
        let tiles: Vec<usize> = grid
            .state
            .borrow()
            .drawn()
            .iter()
            .map(|tile| tile.listing)
            .collect();
        assert!(
            tiles.starts_with(&[3, 5]) && tiles.iter().all(|row| [3, 5, 700].contains(row)),
            "Grid draws only the drawn rows: {tiles:?}"
        );
        shell.view = View::List;
    });
    // Ctrl+A on the typed line stays on it (EY-115); after a cursor key the list's Ctrl+A marks every exact row.
    press(cx, handle, "a", None, true);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.selected.is_empty() && host.filter.owner.typing());
    });
    press(cx, handle, "home", None, false);
    press(cx, handle, "a", None, true);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.selected, vec![3, 5, 700]);
        assert!(!host.filter.owner.typing() && host.filter.owner.up());
    });
    // With the line committed, the cursor keys step through the drawn rows, not the listing.
    press(cx, handle, "j", Some("j"), false);
    assert_eq!(
        edit(cx, handle, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .cursor),
        5
    );
    // Focus.js:187: Escape closes the standing filter and keeps the marks; another Escape clears them.
    press(cx, handle, "escape", None, false);
    assert_eq!(shown(cx, handle), None);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.slots.strips[0].is_none());
        assert_eq!(shell.panes[0].as_ref().unwrap().selected, vec![3, 5, 700]);
    });
    press(cx, handle, "escape", None, false);
    edit(cx, handle, |shell, _, _| {
        assert!(shell.panes[0].as_ref().unwrap().selected.is_empty())
    });
    frame(cx, handle);
    assert_eq!(drawn_list(cx, handle).first(), Some(&(0, 0)));
    // Close rows draw only beside an empty exact list, in rank order, and Ctrl+A never marks one.
    press(cx, handle, "/", Some("/"), false);
    press(cx, handle, "x", Some("x"), false);
    assert_eq!(
        read().as_deref(),
        Some(r#"{"c":"filter","query":"x","id":2}"#)
    );
    reply(
        cx,
        handle,
        r#"{"t":"filtered","id":2,"query":"x","n":1000,"rows":[],"close":[9,4],"ms":0.010,"listing":2}"#,
    );
    assert_eq!(shown(cx, handle), Some(vec![9, 4]));
    press(cx, handle, "home", None, false);
    press(cx, handle, "a", None, true);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.selected,
            Vec::<usize>::new(),
            "the marks the close list hid are pruned and none is added"
        );
        assert_eq!(host.status.as_ref(), "0 of 1,000 shown · 2 close matches");
    });
    press(cx, handle, "j", Some("j"), false);
    assert_eq!(
        edit(cx, handle, |shell, _, _| shell.panes[0]
            .as_ref()
            .unwrap()
            .pane
            .cursor),
        4,
        "a step follows rank order"
    );
    // An answer that breaks the close contract is a protocol error, never half used.
    press(cx, handle, "/", Some("/"), false);
    press(cx, handle, "y", Some("y"), false);
    assert_eq!(
        read().as_deref(),
        Some(r#"{"c":"filter","query":"xy","id":3}"#)
    );
    reply(
        cx,
        handle,
        r#"{"t":"filtered","id":3,"query":"xy","n":1000,"rows":[1],"close":[2],"ms":0.010,"listing":2}"#,
    );
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(
            host.filter.shown.as_deref(),
            Some(&[9, 4][..]),
            "the old answer stands"
        );
        assert!(host.error.is_some(), "the broken answer is reported");
    });
}

fn stamps(cx: &mut TestAppContext, handle: Handle) -> flea::jsondoc::Json {
    flea::jsondoc::parse(ipc(cx, handle, "filterStamps").trim()).unwrap()
}

fn ipc(cx: &mut TestAppContext, handle: Handle, function: &str) -> String {
    let reply = edit(cx, handle, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        let request = flea::ipc::wire::Request::Call {
            target: "flea".into(),
            function: function.into(),
            arguments: vec![],
        };
        shell.ipc(
            crate::ipc::server::Event::Call { request, answer },
            window,
            cx,
        );
        receive.try_recv().unwrap().reply
    });
    assert_eq!(reply.code, 0, "{}", reply.stderr);
    reply.stdout
}

#[gpui::test]
fn filter_stamps_record_the_query_change_then_its_narrowing(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    while read().is_some() {}
    let at = |json: &flea::jsondoc::Json, key: &str| {
        json.get(key).and_then(flea::jsondoc::Json::as_f64).unwrap()
    };
    let idle = stamps(cx, handle);
    assert_eq!((at(&idle, "keyAt"), at(&idle, "narrowedAt")), (0., 0.));
    press(cx, handle, "/", Some("/"), false);
    press(cx, handle, "n", Some("n"), false);
    let typed = stamps(cx, handle);
    assert!(at(&typed, "keyAt") > 0., "the typed key stamps keyAt");
    assert_eq!(
        at(&typed, "narrowedAt"),
        0.,
        "nothing narrowed before the answer"
    );
    reply(
        cx,
        handle,
        r#"{"t":"filtered","id":1,"query":"n","n":1000,"rows":[3,5],"ms":0.010,"listing":2}"#,
    );
    let narrowed = stamps(cx, handle);
    assert_eq!(at(&narrowed, "keyAt"), at(&typed, "keyAt"));
    assert!(at(&narrowed, "narrowedAt") >= at(&narrowed, "keyAt"));
    let view = ipc(cx, handle, "viewMode");
    assert_eq!(
        narrowed.get("view").and_then(flea::jsondoc::Json::as_str),
        Some(view.trim())
    );
}

// Every row a frame draws is held, or the pane has asked for the window that holds it.

#[path = "filter_refill_tests.rs"]
mod refill;
