//! LocalSend discovery, its sends, the empty and gone picks, and the dead-backend path.
use super::*;

#[test]
fn menu_open_asks_peers_and_machines_nothing_runs_before_it() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    assert!(wire.lines.borrow().is_empty());
    let actions = send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    assert_eq!(wire.lines.borrow().len(), 1);
    assert!(wire.lines.borrow()[0].contains(r#""op":"peers""#));
    assert!(send.localsend_checking());
    assert!(send.taildrop_refreshing());
    let run = actions.iter().find_map(|action| match action {
        Action::Run(RunKind::TaildropStatus(_), argv) => Some(argv.clone()),
        _ => None,
    });
    assert_eq!(
        run.expect("a status run").join(" "),
        "timeout --signal=KILL 15 /usr/bin/tailscale status --json"
    );
}
#[test]
fn discovery_rests_warm_but_taildrop_asks_every_open() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 1000);
    send.line(r#"{"t":"localsendpeers","id":1,"peers":[],"reason":"no devices are answering on this network"}"#, &i18n, 1000);
    assert!(!send.localsend_checking());
    wire.lines.borrow_mut().clear();
    let actions = send.menu_opened(&wire, &facts(), Some(&states()), &[], 2000);
    assert!(
        wire.lines.borrow().is_empty(),
        "warm discovery sends nothing"
    );
    assert_eq!(
        status_runs(&actions),
        0,
        "an in-flight status is not asked again, as refresh() returns early"
    );
    let answer = send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    assert_eq!(
        status_runs(&answer),
        1,
        "a menu opened over the run asks again once it lands"
    );
    send.taildrop_done(FIRST_RUN + 1, 0, &ready_status(), "", &i18n);
    send.menu_closed();
    let actions = send.menu_opened(&wire, &facts(), Some(&states()), &[], 3000);
    assert_eq!(
        status_runs(&actions),
        1,
        "every open asks the machines again"
    );
}
#[test]
fn a_stale_peer_reply_is_dropped_and_the_matching_one_lands() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let i18n = english();
    send.line(r#"{"t":"localsendpeers","id":9,"peers":[{"name":"Ghost","address":"10.0.0.9"}],"reason":""}"#, &i18n, 0);
    assert!(send.peers_for(Role::LocalSend).is_empty());
    assert!(send.localsend_checking());
    send.line(
        r#"{"t":"localsendpeers","id":1,"peers":[{"name":"Clean Lemon","address":"192.168.21.23"}],"reason":""}"#,
        &i18n,
        50,
    );
    let peers = send.peers_for(Role::LocalSend);
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].id, "Clean Lemon");
    assert_eq!(peers[0].label, "Clean Lemon");
    assert!(!send.localsend_checking());
}
// r1-4: mn1's refresh::Live reads the answered-once state, which must survive a reopen that asks again.
#[test]
fn an_answered_localsend_stays_answered_while_a_reopen_reads_again() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    assert!(
        !send.localsend_answered(),
        "nothing answered before the first reply"
    );
    send.line(r#"{"t":"localsendpeers","id":1,"peers":[],"reason":"no devices are answering on this network"}"#, &i18n, 0);
    assert!(send.localsend_answered());
    send.menu_closed();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 20_000);
    assert!(
        send.localsend_checking(),
        "the warm window ended, so the list is read again"
    );
    assert!(
        send.localsend_answered(),
        "one answer exists, so the row is not dimmed while it is read"
    );
    send.line(
        r#"{"t":"localsendpeers","id":2,"peers":[],"reason":""}"#,
        &i18n,
        20_000,
    );
    assert!(!send.localsend_checking());
}
#[test]
fn an_uninstalled_localsend_forgets_its_answer_and_its_peers() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.line(r#"{"t":"localsendpeers","id":1,"peers":[{"name":"Clean Lemon","address":"192.168.21.23"}],"reason":""}"#, &i18n, 0);
    assert!(send.localsend_answered());
    let mut gone = facts();
    gone.localsend_command = String::new();
    send.menu_opened(&wire, &gone, Some(&states()), &[], 30_000);
    assert!(!send.localsend_answered());
    assert!(send.peers_for(Role::LocalSend).is_empty());
}
#[test]
fn a_localsend_send_says_its_dispatch_once_then_its_verdict_once() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let i18n = english();
    let paths = vec!["/home/gm/a file.txt".to_string()];
    let actions = send.send_localsend(&wire, "Clean Lemon", &paths, &i18n);
    assert_eq!(
        actions,
        vec![Action::Say(
            "Sending a file.txt to Clean Lemon with LocalSend".into(),
            false
        )]
    );
    assert!(wire.lines.borrow()[1].contains(r#""op":"send""#));
    let said = send.line(
        r#"{"t":"localsendsent","id":2,"ok":true,"reason":""}"#,
        &i18n,
        0,
    );
    assert_eq!(
        said,
        vec![Action::Say(
            "LocalSend finished the transfer.".into(),
            false
        )]
    );
    assert!(send
        .line(
            r#"{"t":"localsendsent","id":2,"ok":false,"reason":"late"}"#,
            &i18n,
            0
        )
        .is_empty());
    assert!(send
        .line(
            r#"{"t":"localsendsent","id":99,"ok":true,"reason":""}"#,
            &i18n,
            0
        )
        .is_empty());
}
// r1-12: a finished send keeps no paths, so a 1,204-file transfer is not held for the window's life.
#[test]
fn a_finished_localsend_send_leaves_the_live_list() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let i18n = english();
    send.send_localsend(&wire, "Clean Lemon", &["/home/gm/a.txt".to_string()], &i18n);
    assert_eq!(send.live_sends(), 1);
    send.line(
        r#"{"t":"localsendsent","id":2,"ok":true,"reason":""}"#,
        &i18n,
        0,
    );
    assert_eq!(send.live_sends(), 0, "a finished send keeps no paths");
}
#[test]
fn an_empty_pick_and_a_dead_wire_each_say_their_sentence() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let i18n = english();
    assert_eq!(
        send.send_localsend(&wire, "Clean Lemon", &[], &i18n),
        vec![Action::Say("There is nothing to send.".into(), true)]
    );
    *wire.dead.borrow_mut() = true;
    let actions = send.send_localsend(&wire, "Clean Lemon", &["/home/gm/a.txt".to_string()], &i18n);
    assert!(
        actions.is_empty(),
        "a dead wire is the host's lost-backend path, never a missing CLI"
    );
}
// r1-11: a dead backend is no missing CLI; the CLI may well be installed, so the bar says nothing here.
#[test]
fn r1_11_a_dead_wire_says_nothing() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    *wire.dead.borrow_mut() = true;
    let actions = send.send_localsend(&wire, "Clean Lemon", &["/home/gm/a.txt".to_string()], &i18n);
    assert!(
        actions.is_empty(),
        "a dead wire was reported as a missing CLI: {actions:?}"
    );
}
// The provider's own reason words a gone CLI; without one the catalog sentence names the CLI.
#[test]
fn a_gone_localsend_cli_says_why_it_left() {
    let i18n = english();
    let paths = ["/home/gm/a.txt".to_string()];
    let mut gone = facts();
    gone.localsend_command = String::new();
    let mut send = ProviderSend::default();
    send.menu_opened(&Fake::default(), &gone, Some(&states()), &[], 0);
    assert_eq!(
        send.send_localsend(&Fake::default(), "Clean Lemon", &paths, &i18n),
        vec![Action::Say(
            "localsend-cli is no longer installed.".into(),
            true
        )]
    );
    gone.localsend_reason = "localsend-cli is not installed.".into();
    let mut send = ProviderSend::default();
    send.menu_opened(&Fake::default(), &gone, Some(&states()), &[], 0);
    assert_eq!(
        send.send_localsend(&Fake::default(), "Clean Lemon", &paths, &i18n),
        vec![Action::Say(
            "LocalSend \u{b7} localsend-cli is not installed.".into(),
            true
        )]
    );
}
// r2-4: LocalSend.qml send() refuses an empty peer and LocalSend.js words it with missing().
#[test]
fn r2_e_an_empty_localsend_peer_writes_nothing() {
    let (mut send, wire, i18n) = (ProviderSend::default(), Fake::default(), english());
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let before = wire.lines.borrow().len();
    let actions = send.send_localsend(&wire, "", &["/home/gm/a.txt".to_string()], &i18n);
    assert_eq!(
        wire.lines.borrow().len(),
        before,
        "an empty peer was queued; said {actions:?}"
    );
    assert_eq!(
        actions,
        vec![Action::Say(
            "localsend-cli is no longer installed.".into(),
            true
        )]
    );
}

#[test]
fn a_dead_backend_ends_discovery_and_loses_live_sends() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let i18n = english();
    send.send_localsend(&wire, "Clean Lemon", &["/home/gm/a.txt".to_string()], &i18n);
    send.backend_dead();
    assert!(!send.localsend_checking());
    assert_eq!(
        send.live_sends(),
        0,
        "the lost send is pruned with its backend"
    );
    assert!(send
        .line(
            r#"{"t":"localsendsent","id":2,"ok":true,"reason":""}"#,
            &i18n,
            0
        )
        .is_empty());
}
