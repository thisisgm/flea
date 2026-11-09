//! The provider join through the live window: two pane backends with overlapping request ids.
use crate::{
    backend::{Backend, Event as BackendEvent},
    g2::context_menus::join::{Captured, Chosen},
    state::{tests::shell_with, Shell},
    window::pane::PaneHost,
};
use flea::protocol::{Reply, RowFacts};
use flea::ui_logic_send_share::providers::Role;
use gpui::{Entity, TestAppContext, VisualTestContext};
use std::{cell::Cell, rc::Rc};

#[path = "activation_tests.rs"]
mod activation_tests;
#[path = "provider_state_tests.rs"]
mod state_tests;

fn right(shell: &Shell, backend: Backend) -> PaneHost {
    let mut host = PaneHost::new("/right".into(), None, 620., &shell.locale);
    host.backend = Some(backend);
    host.pane
        .receive(Reply::Listed {
            path: "/right".into(),
            total: 8,
        })
        .unwrap();
    host.pane
        .receive(Reply::Rows {
            start: 0,
            rows: vec![RowFacts::default(); 8],
            listing: 2,
        })
        .unwrap();
    host
}

fn drain(receive: &mut impl FnMut() -> Option<String>) -> Vec<String> {
    std::iter::from_fn(receive).collect()
}

fn with(lines: &[String], needle: &str) -> Vec<String> {
    lines
        .iter()
        .filter(|line| line.contains(needle))
        .cloned()
        .collect()
}

fn facts(id: u64) -> String {
    format!(
        r#"{{"t":"formats","id":{id},"archive":[],"providers":{{"localsend":{{"installed":true,"command":"/usr/bin/localsend-cli","reason":""}}}}}}"#
    )
}

fn peers(id: u64, name: &str) -> String {
    format!(
        r#"{{"t":"localsendpeers","id":{id},"peers":[{{"name":"{name}","address":"10.0.0.2"}}],"reason":""}}"#
    )
}

fn feed(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize, line: &str) {
    shell.update_in(cx, |shell, window, cx| {
        shell.receive(side, BackendEvent::Producer(line.into()), window, cx)
    });
}

// Opens the file menu on one side and answers its facts ask; returns the menu's capture.
fn open(
    shell: &Entity<Shell>,
    cx: &mut VisualTestContext,
    side: usize,
    receive: &mut impl FnMut() -> Option<String>,
) -> Captured {
    shell.update_in(cx, |shell, window, cx| {
        shell.follow_plugins(window, cx);
        let host = shell.plugins.host.clone().unwrap();
        host.update(cx, |host, _| {
            if host.plugins.is_empty() {
                host.plugins = flea::ui_logic_plugin_settings::builtin::manifests()
                    .into_iter()
                    .map(
                        |(manifest, enabled)| flea::ui_logic_plugin_settings::wire::Plugin {
                            manifest,
                            enabled,
                            source: flea::ui_logic_plugin_settings::wire::Source::Builtin,
                            failure: None,
                            mark_path: String::new(),
                            settings: Vec::new(),
                            shelf: None,
                        },
                    )
                    .collect();
            }
        });
        shell.open_menu(side, Some(0), (10., 10.), window, cx)
    });
    let asked = with(&drain(receive), "\"c\":\"formats\"");
    assert_eq!(
        asked.len(),
        1,
        "a file menu asks its own backend for the providers facts once"
    );
    let id = flea::json::field_usize(&asked[0], "id").unwrap() as u64;
    let captured = shell
        .read_with(cx, |shell, _| shell.menu.captured.clone())
        .expect("menu captured");
    assert_eq!((captured.side, captured.id), (side, id));
    // A facts answer for another menu is not this one's and starts nothing.
    feed(shell, cx, side, &facts(id + 99));
    assert!(
        with(&drain(receive), "\"c\":\"localsend\"").is_empty(),
        "a stale facts reply started discovery"
    );
    feed(shell, cx, side, &facts(id));
    captured
}

fn peer_names(shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize) -> Vec<String> {
    shell.read_with(cx, |shell, _| {
        shell.menu.providers.sides[side]
            .peers_for(Role::LocalSend)
            .into_iter()
            .map(|peer| peer.label)
            .collect()
    })
}

#[gpui::test]
fn m9_integ_two_provider_backends_cannot_consume_each_others_reply(cx: &mut TestAppContext) {
    let (left, mut left_lines) = Backend::test_lines();
    let (other, mut right_lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(left, cx);
        shell.panes[1] = Some(right(&shell, other));
        shell
    });
    let first = open(&shell, cx, 0, &mut left_lines);
    let asked = with(&drain(&mut left_lines), "\"op\":\"peers\"");
    assert_eq!(
        asked,
        vec![r#"{"c":"localsend","op":"peers","peer":"","paths":[],"id":1}"#.to_string()]
    );
    open(&shell, cx, 1, &mut right_lines);
    let asked = with(&drain(&mut right_lines), "\"op\":\"peers\"");
    // Each backend numbers its own requests, so both discovery asks carry id 1.
    assert_eq!(
        asked,
        vec![r#"{"c":"localsend","op":"peers","peer":"","paths":[],"id":1}"#.to_string()]
    );
    assert!(
        drain(&mut left_lines)
            .iter()
            .all(|line| !line.contains("localsend")),
        "side 1's ask reached side 0"
    );

    feed(&shell, cx, 1, &peers(1, "Phone B"));
    assert_eq!(peer_names(&shell, cx, 1), vec!["Phone B".to_string()]);
    assert!(
        peer_names(&shell, cx, 0).is_empty(),
        "side 1's reply landed in side 0's sender"
    );
    assert!(shell.read_with(cx, |shell, _| shell.menu.providers.sides[0]
        .localsend_checking()));
    feed(&shell, cx, 0, &peers(1, "Phone A"));
    assert_eq!(peer_names(&shell, cx, 0), vec!["Phone A".to_string()]);
    assert_eq!(peer_names(&shell, cx, 1), vec!["Phone B".to_string()]);
    // A replayed answer after the ask closed changes nothing.
    feed(&shell, cx, 0, &peers(1, "Ghost"));
    assert_eq!(peer_names(&shell, cx, 0), vec!["Phone A".to_string()]);

    // The checked LocalSend row on side 0 writes its send on side 0's pipe only.
    let claimed = Rc::new(Cell::new(false));
    let chosen = Chosen {
        captured: Captured {
            ready: true,
            ..first
        },
        action: "localsend".into(),
        leaf: "Phone A".into(),
        paths: vec!["/files/a.txt".into()],
        claimed: Rc::clone(&claimed),
    };
    shell.update_in(cx, |_, _, cx| cx.emit(chosen));
    cx.run_until_parked();
    assert!(claimed.get(), "the provider join claims its row");
    let sent = with(&drain(&mut left_lines), "\"op\":\"send\"");
    assert_eq!(
        sent,
        vec![
            r#"{"c":"localsend","op":"send","peer":"Phone A","paths":["/files/a.txt"],"id":2}"#
                .to_string()
        ]
    );
    assert!(with(&drain(&mut right_lines), "\"op\":\"send\"").is_empty());
    let live = |shell: &Entity<Shell>, cx: &mut VisualTestContext, side: usize| {
        shell.read_with(cx, |shell, _| shell.menu.providers.sides[side].live_sends())
    };
    assert_eq!(live(&shell, cx, 0), 1);
    // Side 1's verdict for the same id never ends side 0's send.
    feed(
        &shell,
        cx,
        1,
        r#"{"t":"localsendsent","id":2,"ok":true,"reason":""}"#,
    );
    assert_eq!(
        live(&shell, cx, 0),
        1,
        "side 1's verdict ended side 0's send"
    );
    // Side 1's backend dying loses nothing on side 0.
    shell.update_in(cx, |shell, window, cx| {
        shell.receive(1, BackendEvent::Drained, window, cx)
    });
    assert_eq!(live(&shell, cx, 0), 1);
    feed(
        &shell,
        cx,
        0,
        r#"{"t":"localsendsent","id":2,"ok":true,"reason":""}"#,
    );
    assert_eq!(live(&shell, cx, 0), 0, "the matching verdict ends the send");
    feed(
        &shell,
        cx,
        0,
        r#"{"t":"localsendsent","id":2,"ok":false,"reason":"late"}"#,
    );
    assert_eq!(live(&shell, cx, 0), 0);
}

#[gpui::test]
fn provider_rows_reach_the_menu_only_while_installed_and_their_plugin_is_on(
    cx: &mut TestAppContext,
) {
    let (left, mut lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(left, cx));
    open(&shell, cx, 0, &mut lines);
    feed(&shell, cx, 0, &peers(1, "Phone A"));
    let context = |shell: &Entity<Shell>, cx: &mut VisualTestContext| {
        shell.read_with(cx, |shell, _| {
            let mut context = flea::ui_logic_context_menus::build::Context::default();
            shell.provider_context(0, "/files", &mut context);
            context
        })
    };
    let shown = context(&shell, cx);
    assert!(shown.localsend_installed);
    assert_eq!(shown.localsend_peers.len(), 1);
    assert!(
        !shown.taildrop_installed && !shown.dropbox_installed,
        "uninstalled providers draw no row"
    );
    // The st2 switch off in ui.json hides the row at the next open, with no discovery ask.
    shell.update_in(cx, |shell, _, _| {
        shell.launch.settings.data =
            flea::jsondoc::parse(r#"{"plugins":{"flea.localsend":{"enabled":false}}}"#).unwrap();
    });
    open(&shell, cx, 0, &mut lines);
    assert!(with(&drain(&mut lines), "\"op\":\"peers\"").is_empty());
    let hidden = context(&shell, cx);
    assert!(!hidden.localsend_installed && hidden.localsend_peers.is_empty());
}

// A Taildrop send's own thread reports back: a send that fails to start or exits non-zero leaves its sentence as an error.
#[gpui::test]
fn a_failed_taildrop_send_says_its_sentence_and_a_finished_one_says_nothing(
    cx: &mut TestAppContext,
) {
    // Up to 5 s for the two failures to land, then a settle window long enough for `true` to have exited and reported.
    const POLL_STEPS: usize = 500;
    const POLL_MS: u64 = 10;
    const SETTLE_MS: u64 = 100;
    cx.executor().allow_parking();
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    let sends = vec![
        super::seams::Action::Spawn(vec!["true".into()], "sent true".into()),
        super::seams::Action::Spawn(vec!["false".into()], "sent false".into()),
        super::seams::Action::Spawn(
            vec!["/nonexistent/flea-sd1/no-such-send".into()],
            "sent missing".into(),
        ),
    ];
    let read = |cx: &mut VisualTestContext| {
        shell.update(cx, |shell, cx| {
            shell.status_messages(cx, |messages, _| {
                let mut errors = messages
                    .errors()
                    .iter()
                    .map(|e| e.text.clone())
                    .collect::<Vec<_>>();
                errors.sort();
                (errors, messages.notice_serial())
            })
        })
    };
    let (_, serial) = read(cx);
    shell.update_in(cx, |shell, window, cx| {
        shell.provider_actions(0, sends, window, cx)
    });
    let expected = (
        vec!["sent false".to_string(), "sent missing".to_string()],
        serial,
    );
    for _ in 0..POLL_STEPS {
        cx.run_until_parked();
        if read(cx).0.len() >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
    }
    assert_eq!(
        read(cx),
        expected,
        "both failures land as errors and nothing else is said"
    );
    for _ in 0..SETTLE_MS / POLL_MS {
        std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
        cx.run_until_parked();
        assert_eq!(read(cx), expected, "the finished send said something");
    }
}
