// The provider snapshot reads the focused pane's mounted sender through the live IPC endpoint.
use super::*;
use flea::{ipc::wire::Request, jsondoc::Json};

pub(super) fn state(shell: &Entity<Shell>, cx: &mut VisualTestContext) -> Json {
    let reply = shell.update_in(cx, |shell, window, cx| {
        let (answer, receive) = std::sync::mpsc::sync_channel(1);
        shell.ipc(
            crate::ipc::server::Event::Call {
                request: Request::Call {
                    target: "flea".into(),
                    function: "providerState".into(),
                    arguments: vec![],
                },
                answer,
            },
            window,
            cx,
        );
        receive.try_recv().unwrap().reply
    });
    assert_eq!(reply.code, 0, "{reply:?}");
    flea::jsondoc::parse(&reply.stdout).unwrap()
}

#[gpui::test]
fn provider_state_taildrop_peers_match_the_oracle_contract(cx: &mut TestAppContext) {
    struct Wire;
    impl crate::g2::send_share::seams::Wire for Wire {
        fn send(&self, _: String) -> bool {
            true
        }
    }
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    shell.update_in(cx, |shell, _, _| {
        shell.menu.providers.sides[0].menu_opened(
            &Wire,
            &flea::ui_logic_send_share::providers::Facts {
                tailscale_command: "/usr/bin/tailscale".into(),
                send_command: "/usr/bin/omarchy-tailscale-send".into(),
                ..Default::default()
            },
            None,
            &[],
            0,
        );
    });
    let waiting = state(&shell, cx);
    assert_eq!(
        waiting.get("refreshing").and_then(Json::as_bool),
        Some(true)
    );
    assert_eq!(
        waiting
            .get("taildrop")
            .unwrap()
            .get("checking")
            .and_then(Json::as_bool),
        Some(true)
    );
    shell.update_in(cx, |shell, _, _| {
        shell.menu.providers.sides[0].taildrop_done(
            1,
            0,
            r#"{"BackendState":"Running","Self":{"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{
                "second-peer":{"HostName":"Bravo","DNSName":"fixture.invalid.","Online":true,"TaildropTarget":1},
                "first-peer":{"HostName":"Alpha","DNSName":"unused.invalid.","Online":true,"TaildropTarget":1}}}"#,
            "",
            &shell.locale.catalog,
        );
    });
    let ready = state(&shell, cx);
    assert_eq!(ready.get("refreshing").and_then(Json::as_bool), Some(false));
    let taildrop = ready.get("taildrop").unwrap();
    assert_eq!(
        taildrop.get("checking").and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(taildrop.get("reason").and_then(Json::as_str), Some(""));
    assert_eq!(
        taildrop.get("timeoutSeconds").and_then(Json::as_f64),
        Some(15.)
    );
    let peers = taildrop.get("peers").unwrap().as_array().unwrap();
    assert_eq!(peers.len(), 2);
    for (peer, (id, label, address)) in peers.iter().zip([
        ("first-peer", "Alpha", "unused.invalid"),
        ("second-peer", "Bravo", "fixture.invalid"),
    ]) {
        assert_eq!(peer.get("id").and_then(Json::as_str), Some(id));
        assert_eq!(peer.get("label").and_then(Json::as_str), Some(label));
        assert_eq!(peer.get("address").and_then(Json::as_str), Some(address));
        assert!(peer.get("name").is_none());
    }
}

#[gpui::test]
fn followup_provider_state_reports_selection_identity_and_idle_services(cx: &mut TestAppContext) {
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(backend, cx);
        let host = shell.panes[0].as_mut().unwrap();
        host.selected = vec![0, 99999];
        let mut rows = host.pane.held.rows.clone();
        rows[0].name = "a.txt".into();
        host.pane.held = Rc::new(crate::list::pane::Held {
            rows,
            ..*host.pane.held
        });
        shell
    });
    let value = state(&shell, cx);
    assert_eq!(
        value.get("formatsRequests").and_then(Json::as_f64),
        Some(0.)
    );
    assert_eq!(value.get("refreshing").and_then(Json::as_bool), Some(false));
    assert_eq!(
        value.get("pendingActivation").and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        value.get("cursorPath").and_then(Json::as_str),
        Some("/files/a.txt")
    );
    let paths = value.get("selectedPaths").unwrap().as_array().unwrap();
    assert_eq!(paths[0].as_str(), Some("/files/a.txt"));
    assert_eq!(paths[1], Json::Null);
    let taildrop = value.get("taildrop").unwrap();
    assert_eq!(
        taildrop.get("checking").and_then(Json::as_bool),
        Some(false)
    );
    assert!(taildrop
        .get("peers")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
}

#[gpui::test]
fn followup_provider_state_tracks_formats_and_keeps_panes_separate(cx: &mut TestAppContext) {
    let (backend, mut lines) = Backend::test_lines();
    let (other, _right_lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| {
        let mut shell = shell_with(backend, cx);
        shell.panes[1] = Some(right(&shell, other));
        shell
    });
    shell.update_in(cx, |shell, window, cx| {
        shell.open_menu(0, Some(0), (10., 10.), window, cx)
    });
    let asked = with(&drain(&mut lines), "\"c\":\"formats\"");
    let id = flea::json::field_usize(&asked[0], "id").unwrap() as u64;
    let waiting = state(&shell, cx);
    assert_eq!(
        waiting.get("formatsRequests").and_then(Json::as_f64),
        Some(1.)
    );
    assert_eq!(
        waiting.get("refreshing").and_then(Json::as_bool),
        Some(true)
    );
    feed(&shell, cx, 0, &facts(id + 99));
    assert_eq!(
        state(&shell, cx).get("refreshing").and_then(Json::as_bool),
        Some(true)
    );
    feed(&shell, cx, 0, &facts(id));
    let answered = state(&shell, cx);
    assert_eq!(
        answered.get("refreshing").and_then(Json::as_bool),
        Some(false)
    );
    assert!(answered.get("facts").unwrap().get("localsend").is_some());
    shell.update_in(cx, |shell, _, _| shell.focus_side = 1);
    let other = state(&shell, cx);
    assert_eq!(
        other.get("formatsRequests").and_then(Json::as_f64),
        Some(0.)
    );
    assert!(other.get("facts").unwrap().get("localsend").is_none());
    shell.update_in(cx, |shell, _, _| shell.focus_side = 0);
    shell.update_in(cx, |shell, window, cx| {
        shell.open_menu(0, Some(0), (10., 10.), window, cx)
    });
    assert_eq!(
        state(&shell, cx)
            .get("formatsRequests")
            .and_then(Json::as_f64),
        Some(2.)
    );
}

#[test]
fn followup_provider_dropbox_status_is_a_bounded_captured_job() {
    struct Wire;
    impl crate::g2::send_share::seams::Wire for Wire {
        fn send(&self, _: String) -> bool {
            true
        }
    }
    let mut sender = crate::g2::send_share::ProviderSend::default();
    let facts = flea::ui_logic_send_share::providers::Facts {
        dropbox_command: "/usr/bin/dropbox-cli".into(),
        dropbox_ready: true,
        ..Default::default()
    };
    let actions = sender.menu_opened(&Wire, &facts, None, &[], 0);
    assert!(
        actions.iter().any(|action| matches!(
            action,
            crate::g2::send_share::seams::Action::Run(_, argv)
                if argv == &["timeout", "--signal=KILL", "4", "/usr/bin/dropbox-cli", "status"]
        )),
        "Dropbox readiness must come from a four-second daemon status check: {actions:?}"
    );
}

#[gpui::test]
fn followup_provider_state_counts_other_formats_and_distinguishes_menu_focus(
    cx: &mut TestAppContext,
) {
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|window, cx| {
        let shell = shell_with(backend, cx);
        window.focus(&shell.focus, cx);
        shell
    });
    shell.update_in(cx, |shell, _, _| {
        shell.panes[0]
            .as_ref()
            .unwrap()
            .backend
            .as_ref()
            .unwrap()
            .line(r#"{"c":"formats","id":42}"#.into())
            .unwrap();
    });
    let list = state(&shell, cx);
    assert_eq!(list.get("formatsRequests").and_then(Json::as_f64), Some(1.));
    assert_eq!(list.get("listFocus").and_then(Json::as_bool), Some(true));
    assert_eq!(list.get("menuFocus").and_then(Json::as_bool), Some(false));
    shell.update_in(cx, |shell, window, cx| {
        shell.open_menu(0, Some(0), (10., 10.), window, cx)
    });
    let waiting = state(&shell, cx);
    assert_eq!(
        waiting.get("listFocus").and_then(Json::as_bool),
        Some(false)
    );
    assert_eq!(
        waiting.get("menuFocus").and_then(Json::as_bool),
        Some(false)
    );
    let id = shell.read_with(cx, |shell, _| shell.menu.captured.as_ref().unwrap().id);
    feed(
        &shell,
        cx,
        0,
        &format!(r#"{{"t":"menuaction","id":{id},"op":"snapshot","ok":true,"kinds":[32768]}}"#),
    );
    feed(
        &shell,
        cx,
        0,
        &format!(
            r#"{{"t":"menuaction","id":{id},"op":"applications","ok":true,"applications":[]}}"#
        ),
    );
    let menu = state(&shell, cx);
    assert_eq!(menu.get("formatsRequests").and_then(Json::as_f64), Some(2.));
    assert_eq!(menu.get("listFocus").and_then(Json::as_bool), Some(false));
    assert_eq!(menu.get("menuFocus").and_then(Json::as_bool), Some(true));
}

#[gpui::test]
fn followup_provider_dropbox_move_waits_for_daemon_and_destination(cx: &mut TestAppContext) {
    struct Wire;
    impl crate::g2::send_share::seams::Wire for Wire {
        fn send(&self, _: String) -> bool {
            true
        }
    }
    let (backend, _lines) = Backend::test_lines();
    let (shell, cx) = cx.add_window_view(|_, cx| shell_with(backend, cx));
    let capture = shell.update_in(cx, |shell, window, cx| {
        shell.open_menu(0, Some(0), (10., 10.), window, cx);
        shell.menu.providers.account[0].path = "/Dropbox".into();
        shell.menu.providers.destination[0] = Some(true);
        shell.menu.providers.sides[0].menu_opened(
            &Wire,
            &flea::ui_logic_send_share::providers::Facts {
                dropbox_command: "/usr/bin/dropbox-cli".into(),
                dropbox_ready: true,
                ..Default::default()
            },
            None,
            &[],
            0,
        );
        Captured {
            ready: true,
            ..shell.menu.captured.clone().unwrap()
        }
    });
    let choose = |cx: &mut VisualTestContext| {
        let chosen = Chosen {
            captured: capture.clone(),
            action: "dropbox".into(),
            leaf: String::new(),
            paths: vec!["/files/a.txt".into()],
            claimed: Rc::new(Cell::new(false)),
        };
        shell.update_in(cx, |shell, window, cx| {
            shell.provider_chosen(&chosen, window, cx)
        });
    };
    choose(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.operation_view(0).is_none()),
        "a daemon check in flight must not reach the collision host"
    );
    shell.update_in(cx, |shell, _, _| {
        shell.menu.providers.sides[0].dropbox_done(
            1,
            0,
            "Dropbox isn't running!",
            "",
            &shell.locale.catalog,
        );
    });
    choose(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.operation_view(0).is_none()),
        "a failed daemon check cannot authorize a move"
    );
    shell.update_in(cx, |shell, _, _| {
        let sender = &mut shell.menu.providers.sides[0];
        sender.dropbox_refresh();
        sender.dropbox_done(2, 0, "Up to date", "", &shell.locale.catalog);
        shell.menu.providers.destination[0] = Some(false);
    });
    assert_eq!(
        state(&shell, cx)
            .get("dropbox")
            .unwrap()
            .get("ready")
            .and_then(Json::as_bool),
        Some(true),
        "daemon readiness remains distinct from destination validation"
    );
    choose(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.operation_view(0).is_none()),
        "a rejected destination cannot authorize a move"
    );
    shell.update_in(cx, |shell, _, _| {
        shell.menu.providers.destination[0] = Some(true)
    });
    choose(cx);
    assert!(
        shell.read_with(cx, |shell, _| shell.operation_view(0).is_some()),
        "a healthy daemon and accepted destination reach the ordinary collision host"
    );
}
