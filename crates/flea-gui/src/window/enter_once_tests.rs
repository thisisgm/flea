//! One Enter on the f and / query lines commits the line and acts on the highlighted row: a file opens in its default app, a folder opens.
use super::tests::{edit, frame, key, open, reply, Handle};
use crate::{backend::Event as BackendEvent, state::Shell, window::dispatch::test_shell::land};
use flea::protocol::RowFacts;
use gpui::{TestAppContext, VisualTestContext};
use std::cell::RefCell;

thread_local! {
    static OPENED: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
}

// The test launcher: records the argv the opener hands flea --open, never starting a process.
fn record(argv: &[String]) -> crate::g2::open_with_properties::lane::Exit {
    OPENED.with(|opened| opened.borrow_mut().push(argv.to_vec()));
    (Some(0), String::new())
}

// The paths the default app was handed since the last call.
fn launched(cx: &mut TestAppContext) -> Vec<String> {
    cx.run_until_parked();
    OPENED
        .with(|opened| std::mem::take(&mut *opened.borrow_mut()))
        .into_iter()
        .map(|argv| argv[2..].join(" "))
        .collect()
}

fn recorded(cx: &mut TestAppContext, handle: Handle) {
    OPENED.with(|opened| opened.borrow_mut().clear());
    edit(cx, handle, |shell, _, _| shell.opener[0].launch = record);
}

fn file(name: &str) -> RowFacts {
    RowFacts {
        name: name.into(),
        icon: "text-plain".into(),
        ..Default::default()
    }
}

fn folder(name: &str) -> RowFacts {
    RowFacts {
        name: name.into(),
        dir: true,
        icon: "folder".into(),
        ..Default::default()
    }
}

// The listing the named folder lands as, after the fixture's own listing 2.
const LISTING: usize = 3;

/// /files lands as notes.txt, the nodes folder and zeta.txt, which the filter's n leaves out.
fn named_listing(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    let shell = handle.update(cx, |root, _, _| root.shell.clone()).unwrap();
    land(
        &shell,
        &mut VisualTestContext::from_window(handle.into(), cx),
        "/files",
        vec![file("notes.txt"), folder("nodes"), file("zeta.txt")],
        LISTING,
    );
    recorded(cx, handle);
    while read().is_some() {}
    (handle, read)
}

fn plain_listing(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    recorded(cx, handle);
    while read().is_some() {}
    (handle, read)
}

fn drain(read: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    std::iter::from_fn(read).collect()
}

fn lists(lines: &[String], path: &str) -> bool {
    lines.iter().any(|line| {
        flea::json::field_str(line, "c").as_deref() == Some("list")
            && flea::json::field_str(line, "path").as_deref() == Some(path)
    })
}

fn cursor_to(cx: &mut TestAppContext, handle: Handle, row: usize) {
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_mut().unwrap().pane.cursor = row
    });
}

fn filter_typed(
    cx: &mut TestAppContext,
    handle: Handle,
    read: &mut impl FnMut() -> Option<String>,
) {
    key(cx, handle, "/", Some("/"));
    key(cx, handle, "n", Some("n"));
    assert_eq!(
        read().as_deref(),
        Some(r#"{"c":"filter","query":"n","id":1}"#)
    );
}

fn filter_answer(cx: &mut TestAppContext, handle: Handle) {
    reply(
        cx,
        handle,
        0,
        r#"{"t":"filtered","id":1,"query":"n","n":3,"rows":[0,1],"ms":0.010,"listing":3}"#,
    );
}

fn revealed(cx: &mut TestAppContext, handle: Handle) -> (bool, Option<String>) {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.filter.owner.up(), host.nav.pending_select.clone())
    })
}

// S11J-1, board 0401-find-results: one Enter opens a file in its default app and never reveals it in its folder.
#[gpui::test]
fn one_enter_on_the_filter_line_opens_the_highlighted_file(cx: &mut TestAppContext) {
    let (handle, mut read) = named_listing(cx);
    filter_typed(cx, handle, &mut read);
    filter_answer(cx, handle);
    key(cx, handle, "enter", None);
    let lines = drain(&mut read);
    assert_eq!(launched(cx), ["/files/notes.txt"], "{lines:?}");
    assert_eq!(
        revealed(cx, handle),
        (true, None),
        "the filter stands and nothing is revealed: {lines:?}"
    );
    assert!(!lists(&lines, "/files"), "{lines:?}");
}

#[gpui::test]
fn one_enter_on_the_filter_line_enters_the_highlighted_folder(cx: &mut TestAppContext) {
    let (handle, mut read) = named_listing(cx);
    cursor_to(cx, handle, 1);
    filter_typed(cx, handle, &mut read);
    filter_answer(cx, handle);
    key(cx, handle, "enter", None);
    let lines = drain(&mut read);
    assert!(lists(&lines, "/files/nodes"), "{lines:?}");
}

#[gpui::test]
fn an_enter_ahead_of_the_filter_answer_acts_when_it_lands(cx: &mut TestAppContext) {
    let (handle, mut read) = named_listing(cx);
    filter_typed(cx, handle, &mut read);
    key(cx, handle, "enter", None);
    edit(cx, handle, |shell, _, _| {
        let owner = &shell.panes[0].as_ref().unwrap().filter.owner;
        assert!(
            owner.up() && !owner.typing(),
            "Enter commits the line at once"
        );
    });
    filter_answer(cx, handle);
    let lines = drain(&mut read);
    assert_eq!(launched(cx), ["/files/notes.txt"], "{lines:?}");
    assert_eq!(revealed(cx, handle), (true, None), "{lines:?}");
}

#[gpui::test]
fn a_key_after_a_waiting_filter_enter_takes_the_keyboard_back(cx: &mut TestAppContext) {
    let (handle, mut read) = named_listing(cx);
    filter_typed(cx, handle, &mut read);
    key(cx, handle, "enter", None);
    key(cx, handle, "down", None);
    filter_answer(cx, handle);
    let lines = drain(&mut read);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.filter.owner.up(), "{lines:?}");
        assert_eq!(host.nav.pending_select, None);
    });
    assert!(!lists(&lines, "/files"), "{lines:?}");
    assert!(launched(cx).is_empty(), "the dropped Enter opens nothing");
}

fn search_typed(cx: &mut TestAppContext, handle: Handle) -> usize {
    key(cx, handle, "f", Some("f"));
    key(cx, handle, "n", Some("n"));
    edit(cx, handle, |shell, _, _| {
        shell.panes[0].as_ref().unwrap().search.owner.session.gen as usize
    })
}

fn search_answer(cx: &mut TestAppContext, handle: Handle, generation: usize) {
    reply(
        cx,
        handle,
        0,
        &format!(r#"{{"t":"listed","path":"/files","n":2,"gen":{generation}}}"#),
    );
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            BackendEvent::Session {
                generation,
                reply: flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 10 + generation,
                    rows: vec![file("notes.txt"), folder("nodes")],
                },
            },
            window,
            cx,
        )
    });
    reply(
        cx,
        handle,
        0,
        &format!(
            r#"{{"t":"searched","n":2,"scanned":11,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#
        ),
    );
    frame(cx, handle);
}

fn search_left(cx: &mut TestAppContext, handle: Handle) -> (bool, Option<String>) {
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        (host.search.owner.session.leaving, host.select.clone())
    })
}

#[gpui::test]
fn one_enter_on_the_search_line_opens_the_highlighted_file(cx: &mut TestAppContext) {
    let (handle, mut read) = plain_listing(cx);
    let generation = search_typed(cx, handle);
    search_answer(cx, handle, generation);
    drain(&mut read);
    key(cx, handle, "enter", None);
    let lines = drain(&mut read);
    assert_eq!(launched(cx), ["/files/notes.txt"], "{lines:?}");
    edit(cx, handle, |shell, _, _| {
        let owner = &shell.panes[0].as_ref().unwrap().search.owner;
        assert!(
            owner.active() && !owner.typing() && !owner.session.leaving,
            "the results stay, so the next hit is one key away: {lines:?}"
        );
    });
}

#[gpui::test]
fn one_enter_on_the_search_line_opens_the_highlighted_folder(cx: &mut TestAppContext) {
    let (handle, mut read) = plain_listing(cx);
    let generation = search_typed(cx, handle);
    search_answer(cx, handle, generation);
    cursor_to(cx, handle, 1);
    drain(&mut read);
    key(cx, handle, "enter", None);
    let lines = drain(&mut read);
    assert!(lists(&lines, "/files/nodes"), "{lines:?}");
}

#[gpui::test]
fn an_enter_ahead_of_the_search_rows_acts_when_they_land(cx: &mut TestAppContext) {
    let (handle, mut read) = plain_listing(cx);
    let generation = search_typed(cx, handle);
    key(cx, handle, "enter", None);
    edit(cx, handle, |shell, _, _| {
        let owner = &shell.panes[0].as_ref().unwrap().search.owner;
        assert!(
            owner.active() && !owner.typing(),
            "Enter commits the line at once"
        );
    });
    search_answer(cx, handle, generation);
    let lines = drain(&mut read);
    assert_eq!(launched(cx), ["/files/notes.txt"], "{lines:?}");
    assert_eq!(search_left(cx, handle), (false, None), "{lines:?}");
}

#[gpui::test]
fn a_key_after_a_waiting_search_enter_takes_the_keyboard_back(cx: &mut TestAppContext) {
    let (handle, mut read) = plain_listing(cx);
    let generation = search_typed(cx, handle);
    key(cx, handle, "enter", None);
    key(cx, handle, "down", None);
    search_answer(cx, handle, generation);
    let lines = drain(&mut read);
    edit(cx, handle, |shell, _, _| {
        let owner = &shell.panes[0].as_ref().unwrap().search.owner;
        assert!(owner.active() && !owner.session.leaving, "{lines:?}");
    });
}

#[gpui::test]
fn a_press_after_a_waiting_search_enter_takes_the_keyboard_back(cx: &mut TestAppContext) {
    let (handle, mut read) = plain_listing(cx);
    let generation = search_typed(cx, handle);
    key(cx, handle, "enter", None);
    edit(cx, handle, |shell, window, cx| {
        let waiting = |shell: &Shell| shell.panes[0].as_ref().unwrap().search.enter_after;
        assert!(waiting(shell).is_some(), "the Enter waits on its rows");
        shell.pointer_down(0, &gpui::MouseDownEvent::default(), window, cx);
        assert_eq!(waiting(shell), None, "the press itself drops it");
    });
    search_answer(cx, handle, generation);
    let lines = drain(&mut read);
    edit(cx, handle, |shell, _, _| {
        let owner = &shell.panes[0].as_ref().unwrap().search.owner;
        assert!(owner.active() && !owner.session.leaving, "{lines:?}");
    });
}

#[gpui::test]
fn a_waiting_enter_never_acts_while_another_owner_holds_the_keyboard(cx: &mut TestAppContext) {
    let (handle, mut read) = plain_listing(cx);
    let generation = search_typed(cx, handle);
    key(cx, handle, "enter", None);
    edit(cx, handle, |shell, _, _| {
        shell.keyboard.context = flea::keys::Context::Rail
    });
    search_answer(cx, handle, generation);
    let lines = drain(&mut read);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.search.owner.session.leaving, "{lines:?}");
        assert_eq!(
            host.search.enter_after, None,
            "the dropped Enter does not wait on"
        );
    });
}

#[gpui::test]
fn down_keeps_the_search_line_typing_and_one_enter_opens_the_row_it_reached(
    cx: &mut TestAppContext,
) {
    let (handle, mut read) = plain_listing(cx);
    let generation = search_typed(cx, handle);
    search_answer(cx, handle, generation);
    key(cx, handle, "down", None);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.search.owner.typing(), "letters still type after Down");
        assert_eq!(host.pane.cursor, 1);
    });
    drain(&mut read);
    key(cx, handle, "enter", None);
    let lines = drain(&mut read);
    assert!(lists(&lines, "/files/nodes"), "{lines:?}");
}
