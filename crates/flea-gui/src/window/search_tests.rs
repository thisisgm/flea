//! Mounted join controls: real Shell keys, typed backend events, pane ownership and List paint.
use super::*;
use crate::{
    backend::{Backend, Event as BackendEvent},
    ipc::server::Event,
    state::tests::shell_with,
    window::{root::Root, startup::View},
};
use flea::{
    ipc::wire::{Reply, Request},
    protocol::RowFacts,
};
use gpui::{AppContext, KeyDownEvent, Keystroke, Modifiers, TestAppContext, WindowHandle};
use std::time::Duration;

pub(super) type Handle = WindowHandle<Root>;

pub(super) fn open(cx: &mut TestAppContext) -> (Handle, impl FnMut() -> Option<String>) {
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

pub(super) fn edit<R>(
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

pub(super) fn key(cx: &mut TestAppContext, handle: Handle, key: &str, text: Option<&str>) {
    edit(cx, handle, |shell, window, cx| {
        shell.key(
            &KeyDownEvent {
                keystroke: Keystroke {
                    key: key.into(),
                    key_char: text.map(Into::into),
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

pub(super) fn control_key(cx: &mut TestAppContext, handle: Handle, key: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.key(
            &KeyDownEvent {
                keystroke: Keystroke {
                    key: key.into(),
                    modifiers: Modifiers {
                        control: true,
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

/// The committed results with no row acted on, as a tag place opens them; Enter on the line goes to its row instead.
pub(super) fn results(cx: &mut TestAppContext, handle: Handle) {
    edit(cx, handle, |shell, window, cx| {
        shell.search_update(
            0,
            None,
            |search, host| search.key(crate::g2::file_search::StripKey::Enter, host),
            window,
            cx,
        )
    });
}

pub(super) fn reply(cx: &mut TestAppContext, handle: Handle, side: usize, line: &str) {
    edit(cx, handle, |shell, window, cx| {
        shell.receive(side, Backend::test_reply_line(line), window, cx)
    });
}

fn answer(
    cx: &mut TestAppContext,
    handle: Handle,
    side: usize,
    generation: usize,
    name: Option<&str>,
) {
    reply(
        cx,
        handle,
        side,
        &format!(
            r#"{{"t":"listed","path":"/files","n":{},"gen":{generation}}}"#,
            usize::from(name.is_some())
        ),
    );
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            side,
            BackendEvent::Session {
                generation,
                reply: flea::protocol::Reply::Rows {
                    start: 0,
                    listing: 10 + generation,
                    rows: name
                        .into_iter()
                        .map(|name| RowFacts {
                            name: name.into(),
                            icon: "text-plain".into(),
                            ..Default::default()
                        })
                        .collect(),
                },
            },
            window,
            cx,
        )
    });
    reply(
        cx,
        handle,
        side,
        &format!(
            r#"{{"t":"searched","n":{},"scanned":11,"ms":1,"cancelled":false,"gen":{generation},"walking":false}}"#,
            usize::from(name.is_some())
        ),
    );
}

pub(super) fn call(cx: &mut TestAppContext, handle: Handle, function: &str) -> Reply {
    edit(cx, handle, |shell, window, cx| {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        shell.ipc(
            Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: function.into(),
                    arguments: vec![],
                },
                answer: send,
            },
            window,
            cx,
        );
        receive.try_recv().unwrap().reply
    })
}

pub(super) fn frame(cx: &mut TestAppContext, handle: Handle) {
    cx.update_window(handle.into(), |_, window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn mounted_search_key_sends_to_existing_backend_and_exposes_actual_query(cx: &mut TestAppContext) {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    let startup = [read(), read(), read()];
    assert_eq!(
        startup,
        [
            Some(r#"{"c":"fsinfo"}"#.to_owned()),
            Some(r#"{"c":"fsinfo"}"#.to_owned()),
            None,
        ],
        "the size planner and footer each ask once before search; actual startup lines: {startup:?}",
    );
    key(cx, handle, "f", Some("f"));
    let opened =
        read().expect("the search key must enqueue searchopen after the checked startup prefix");
    assert_eq!(
        opened, r#"{"c":"searchopen","path":"/files","folder":"/files","hidden":false}"#,
        "actual request after the search key: {opened}",
    );
    key(cx, handle, "n", Some("n"));
    let sent = read().expect("a query character must enqueue searchquery");
    assert_eq!(
        flea::json::field_str(&sent, "c").as_deref(),
        Some("searchquery"),
        "actual request after the query key: {sent}",
    );
    assert!(
        sent.contains(r#""query":"n""#),
        "actual query request: {sent}"
    );
    let query = call(cx, handle, "keyDeliveryState");
    assert_eq!(query.code, 0);
    assert!(query.stdout.contains(r#""searchQuery":"n""#));
    assert_eq!(call(cx, handle, "searchStamps").code, 0);
}

#[gpui::test]
fn mounted_ctrl_g_turns_the_strip_to_contents_and_sends_only_contents_requests(
    cx: &mut TestAppContext,
) {
    let (handle, mut read) = open(cx);
    frame(cx, handle);
    let _startup = [read(), read(), read()];
    key(cx, handle, "f", Some("f"));
    let opened = read().expect("the search key must enqueue searchopen");
    assert_eq!(
        flea::json::field_str(&opened, "c").as_deref(),
        Some("searchopen"),
        "actual request after the search key: {opened}",
    );
    control_key(cx, handle, "g");
    edit(cx, handle, |shell, _, _| {
        let session = &shell.panes[0].as_ref().unwrap().search.owner.session;
        assert_eq!(
            session.kind,
            flea::ui_logic_file_search::Kind::Contents,
            "Ctrl+G must switch the strip to contents",
        );
    });
    for (key_name, text) in [("F", "F"), ("l", "l"), ("e", "e"), ("a", "a")] {
        key(cx, handle, key_name, Some(text));
    }
    let sent: Vec<String> = std::iter::from_fn(&mut read).collect();
    assert_eq!(
        sent.len(),
        4,
        "one contents request per query key: {sent:?}"
    );
    for line in &sent {
        assert_eq!(
            flea::json::field_str(line, "c").as_deref(),
            Some("searchcontents"),
            "a contents strip never sends a names request: {line}",
        );
    }
    assert_eq!(
        flea::json::field_str(&sent[3], "query").as_deref(),
        Some("Flea"),
        "actual contents query: {}",
        sent[3],
    );
}

#[gpui::test]
fn mounted_transport_accepts_only_current_search_generation(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    key(cx, handle, "f", Some("f"));
    key(cx, handle, "a", Some("a"));
    key(cx, handle, "b", Some("b"));
    answer(cx, handle, 0, 2, Some("Docs/ab.txt"));
    answer(cx, handle, 0, 1, Some("stale.txt"));
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert_eq!(host.search.rows_generation, 2);
        assert_eq!(host.pane.held.rows[0].name, "Docs/ab.txt");
        assert_eq!(host.pane.swap.session.query, "ab");
    });
}

#[gpui::test]
fn mounted_search_replies_do_not_cross_panes(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        let mut right =
            super::super::pane::PaneHost::new("/right".into(), None, 620., &shell.locale);
        right.backend = Some(Backend::test_lines().0);
        shell.panes[1] = Some(right);
        shell.lifecycle.attach(1);
        shell.view = View::Dual;
        shell.focus_side = 0;
        shell.search_action("search", window, cx);
    });
    key(cx, handle, "x", Some("x"));
    reply(
        cx,
        handle,
        1,
        r#"{"t":"searched","n":99,"scanned":99,"ms":1,"cancelled":false,"gen":1,"walking":false}"#,
    );
    edit(cx, handle, |shell, _, _| {
        assert!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .search
                .owner
                .session
                .running
        );
        assert_eq!(
            shell.panes[0].as_ref().unwrap().search.owner.session.query,
            "x"
        );
        assert!(!shell.panes[1].as_ref().unwrap().search.owner.active());
    });
}

#[gpui::test]
fn mounted_escape_publishes_leaving_and_failure_closes_the_same_owner(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    key(cx, handle, "f", Some("f"));
    key(cx, handle, "x", Some("x"));
    answer(cx, handle, 0, 1, Some("x.txt"));
    key(cx, handle, "escape", None);
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.pane.swap.session.leaving);
        assert!(host.pane.swap.session.drops(1));
        assert!(host.pane.swap.holding);
    });
    edit(cx, handle, |shell, window, cx| {
        shell.receive(
            0,
            BackendEvent::Failed(crate::backend::Failure {
                key: "window.backend_eof",
                detail: String::new(),
            }),
            window,
            cx,
        )
    });
    edit(cx, handle, |shell, _, _| {
        let host = shell.panes[0].as_ref().unwrap();
        assert!(!host.search.owner.active());
        assert!(!host.pane.swap.session.open);
    });
}

#[gpui::test]
fn mounted_back_while_walking_arms_one_held_relist(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    key(cx, handle, "f", Some("f"));
    key(cx, handle, "x", Some("x"));
    edit(cx, handle, |shell, window, cx| {
        assert!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .search
                .owner
                .session
                .running
        );
        assert_eq!(
            shell.step(0, super::super::navigate::Step::Back, window, cx),
            Ok(true)
        );
        let host = shell.panes[0].as_ref().unwrap();
        assert!(host.search.owner.relisting());
        assert!(host.pane.swap.session.leaving && host.pane.swap.holding);
        assert_eq!(host.nav.counters.lists, 1);
    });
}

#[gpui::test]
fn mounted_old_held_rows_never_stamp_the_new_query(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    key(cx, handle, "f", Some("f"));
    key(cx, handle, "a", Some("a"));
    answer(cx, handle, 0, 1, Some("a.txt"));
    frame(cx, handle);
    key(cx, handle, "b", Some("b"));
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        assert_eq!(
            shell.panes[0]
                .as_ref()
                .unwrap()
                .search
                .owner
                .session
                .stamps
                .first_rows_at,
            0
        )
    });
    cx.executor().advance_clock(Duration::from_millis(10));
    answer(cx, handle, 0, 2, Some("ab.txt"));
    frame(cx, handle);
    edit(cx, handle, |shell, _, _| {
        let stamps = shell.panes[0].as_ref().unwrap().search.owner.session.stamps;
        assert_eq!(stamps.gen, 2);
        assert!(stamps.first_rows_at >= stamps.key_at && stamps.first_rows_at > 0);
    });
}

#[gpui::test]
fn mounted_search_uses_header_slot_and_keeps_columns(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    edit(cx, handle, |shell, window, cx| {
        shell.view = View::Columns;
        assert!(shell.search_action("search", window, cx));
        assert_eq!(
            shell.view,
            View::Columns,
            "HIG cols4-hero-search: Columns searches in place"
        );
        assert!(shell.search_header(0, window, cx).is_some());
    });
    key(cx, handle, "escape", None);
    edit(cx, handle, |shell, _, _| {
        assert_eq!(shell.view, View::Columns)
    });
}

#[gpui::test]
fn mounted_search_does_not_take_another_editors_keyboard(cx: &mut TestAppContext) {
    let (handle, _read) = open(cx);
    key(cx, handle, "f", Some("f"));
    edit(cx, handle, |shell, window, cx| {
        let editor = cx.focus_handle();
        window.focus(&editor, cx);
        let event = KeyDownEvent {
            keystroke: Keystroke {
                key: "x".into(),
                key_char: Some("x".into()),
                ..Default::default()
            },
            is_held: false,
            prefer_character_input: false,
        };
        assert!(!shell.search_key(&event, window, cx));
        assert!(shell.panes[0]
            .as_ref()
            .unwrap()
            .search
            .owner
            .session
            .query
            .is_empty());
    });
}
#[path = "search_density_tests.rs"]
mod density;

#[path = "search_uish_tests.rs"]
mod uish;
