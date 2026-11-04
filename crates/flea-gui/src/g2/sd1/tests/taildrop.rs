//! Taildrop status runs, their re-asks, the stale and checking refusals, and the timed-out run.
use super::*;

#[test]
fn uish_misc_activation_refresh_asks_only_the_chosen_provider() {
    for role in [Role::Tailscale, Role::Dropbox] {
        let mut sender = ProviderSend::default();
        let wire = Fake::default();
        let actions = sender.menu_refresh(&wire, &facts(), Some(&states()), &[], 0, Some(role));
        assert!(
            wire.lines.borrow().is_empty(),
            "activation must not discover LocalSend"
        );
        assert_eq!(actions.len(), 1, "{role:?}: {actions:?}");
        assert!(matches!(
            (&role, &actions[0]),
            (Role::Tailscale, Action::Run(RunKind::TaildropStatus(_), _))
                | (Role::Dropbox, Action::Run(RunKind::DropboxStatus(_), _))
        ));
    }
}

#[test]
fn uish_misc_activation_waits_for_a_new_status_after_an_existing_run() {
    let mut sender = ProviderSend::default();
    let wire = Fake::default();
    sender.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let actions = sender.menu_refresh(
        &wire,
        &facts(),
        Some(&states()),
        &[],
        1,
        Some(Role::Tailscale),
    );
    assert!(actions.is_empty());
    assert!(sender.taildrop_refreshing());
    let actions = sender.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &english());
    assert_eq!(status_runs(&actions), 1);
    assert!(sender.taildrop_refreshing());
    assert!(
        sender.peers_for(Role::Tailscale).is_empty(),
        "the old answer cannot validate activation"
    );
    sender.taildrop_done(FIRST_RUN + 1, 0, &ready_status(), "", &english());
    assert!(!sender.taildrop_refreshing());
    assert_eq!(sender.peers_for(Role::Tailscale).len(), 1);
}

// r1-1: one status run at a time, as ui/Taildrop.qml holds one statusProcess; a reopen waits for it.
#[test]
fn r1_1_status_runs_never_overlap() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let first = send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    assert_eq!(status_runs(&first), 1);
    send.menu_closed();
    let second = send.menu_opened(&wire, &facts(), Some(&states()), &[], 3000);
    assert_eq!(
        status_runs(&second),
        0,
        "two tailscale status runs are in flight at once"
    );
}

// r1-1: the run that landed answers once, its repeat is dropped, and the re-ask's answer is the one that stands.
#[test]
fn r1_1b_the_fresh_answer_stands() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.menu_closed();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 3000);
    let asked_again = send.taildrop_done(FIRST_RUN, 0, r#"{"BackendState":"Stopped"}"#, "", &i18n);
    assert_eq!(
        status_runs(&asked_again),
        1,
        "the run that landed is the only one, so the waiting open asks next"
    );
    let late = send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    assert!(
        late.is_empty(),
        "an answer from a run that already landed is dropped"
    );
    assert!(send.taildrop_refreshing(), "the re-ask is still in flight");
    assert!(
        send.peers_for(Role::Tailscale).is_empty(),
        "the stale ready answer did not reach the flyout"
    );
    send.taildrop_done(FIRST_RUN + 1, 0, &ready_status(), "", &i18n);
    assert_eq!(send.taildrop_code(), "", "the fresh answer was applied");
    assert_eq!(send.peers_for(Role::Tailscale).len(), 1);
}

// r1-1: a close does not cancel the run; its answer still lands for the next open.
#[test]
fn menu_closed_leaves_the_run_to_land() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.menu_closed();
    assert!(
        send.taildrop_refreshing(),
        "the status service outlives the menu"
    );
    send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    assert_eq!(send.peers_for(Role::Tailscale).len(), 1);
}

// r1-2: a send script that left leaves no peer rows from an earlier open.
#[test]
fn r1_2_a_missing_send_script_leaves_no_peers() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    assert_eq!(send.peers_for(Role::Tailscale).len(), 1);
    let mut gone = facts();
    gone.send_command = String::new();
    send.menu_opened(&wire, &gone, Some(&states()), &[], 10_000);
    assert!(
        send.peers_for(Role::Tailscale).is_empty(),
        "the flyout still offers the last open's peers"
    );
    assert_eq!(send.taildrop_code(), "");
}
// r1-3: only the cursor row's file is sent; a pick of two files refuses as stale and never spawns.
#[test]
fn r1_3_two_picks_never_spawn() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    let paths = vec!["/home/gm/a.txt".to_string(), "/home/gm/b.txt".to_string()];
    let actions = send.send_taildrop("nodekey:owned", &paths, &i18n);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Spawn(..))),
        "a two-file pick spawned {actions:?}"
    );
    assert_eq!(
        actions,
        vec![Action::Say(
            "That menu is out of date. Reopen it.".into(),
            true
        )]
    );
}

// r1-3: an empty ShareLink pick is the stale-menu sentence, not the no-file one.
#[test]
fn r1_3b_an_empty_share_link_pick_is_menu_stale() {
    let mut send = ProviderSend::default();
    send.menu_opened(&Fake::default(), &facts(), Some(&states()), &[], 0);
    let i18n = english();
    assert_eq!(
        send.copy_share_link("", &i18n),
        vec![Action::Say(
            "That menu is out of date. Reopen it.".into(),
            true
        )]
    );
}
#[test]
fn a_taildrop_send_carries_its_failure_sentence_and_a_stale_peer_refuses() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let i18n = english();
    assert!(send
        .taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n)
        .is_empty());
    assert!(!send.taildrop_refreshing());
    assert_eq!(send.taildrop_code(), "");
    let paths = vec!["/home/gm/a.txt".to_string()];
    let actions = send.send_taildrop("nodekey:owned", &paths, &i18n);
    assert_eq!(
        actions[0],
        Action::Spawn(
            vec![
                "/usr/bin/omarchy-tailscale-send".into(),
                "laptop.ts.net".into(),
                "/home/gm/a.txt".into()
            ],
            "Taildrop could not send a.txt to laptop.".into()
        )
    );
    assert_eq!(
        actions[1],
        Action::Say("Sending a.txt to laptop.".into(), false)
    );
    let refused = send.send_taildrop("nodekey:gone", &paths, &i18n);
    assert_eq!(
        refused,
        vec![Action::Say(
            "That Taildrop peer is no longer available.".into(),
            true
        )]
    );
    let stale = send.send_taildrop("nodekey:owned", &["relative.txt".to_string()], &i18n);
    assert_eq!(
        stale,
        vec![Action::Say(
            "That menu is out of date. Reopen it.".into(),
            true
        )]
    );
}
// r1-10: a pick while the status runs says the checking word, as ui/js/Ops.js does.
#[test]
fn r1_10b_a_pick_while_the_status_runs_says_checking() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 10_000);
    assert!(send.taildrop_refreshing());
    let actions = send.send_taildrop("nodekey:owned", &["/home/gm/a.txt".to_string()], &i18n);
    assert_eq!(
        actions,
        vec![Action::Say("Taildrop \u{b7} checking".into(), true)]
    );
}

// r1-10: an empty status diagnostic names the check through the catalog, not as English in core.
#[test]
fn r1_10_an_empty_status_failure_names_the_check() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.taildrop_done(FIRST_RUN, 1, "", "", &i18n);
    assert_eq!(send.taildrop_code(), "statusFailed");
    let actions = send.send_taildrop("nodekey:owned", &["/home/gm/a.txt".to_string()], &i18n);
    assert_eq!(
        actions,
        vec![Action::Say(
            "Taildrop \u{b7} Tailscale could not be checked".into(),
            true
        )]
    );
}

// r1-8: a peer that carries only an address still draws a word, the reserved catalog one.
#[test]
fn r1_8_an_address_only_peer_is_named() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.taildrop_done(FIRST_RUN, 0, &address_only_status(), "", &i18n);
    let peers = send.peers_for(Role::Tailscale);
    assert_eq!(peers.len(), 1);
    assert_eq!(
        peers[0].label, "Unknown",
        "an address-only peer is drawn as an empty row"
    );
}

#[test]
fn a_timed_out_status_run_reads_as_a_failure_with_its_code() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let i18n = english();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    assert!(send.taildrop_done(FIRST_RUN, 137, "", "", &i18n).is_empty());
    assert_eq!(send.taildrop_code(), "statusFailed");
    assert_eq!(
        send.send_taildrop("nodekey:owned", &["/home/gm/a.txt".to_string()], &i18n),
        vec![Action::Say(
            "Taildrop \u{b7} Tailscale did not answer in time".into(),
            true
        )]
    );
    assert!(send.peers_for(Role::Tailscale).is_empty());
    assert!(
        send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n)
            .is_empty(),
        "a second report answers nothing"
    );
    assert_eq!(
        send.taildrop_code(),
        "statusFailed",
        "a wrongly applied duplicate replaced the failed code"
    );
    assert!(
        send.peers_for(Role::Tailscale).is_empty(),
        "a wrongly applied duplicate added its peer"
    );
}

// r2-3: the status run is a host process, so a backend death neither starts a second run nor drops its answer.
#[test]
fn r2_f_a_backend_death_never_starts_a_second_status_run() {
    let (mut send, wire) = (ProviderSend::default(), Fake::default());
    assert_eq!(
        status_runs(&send.menu_opened(&wire, &facts(), Some(&states()), &[], 0)),
        1
    );
    send.backend_dead();
    let again = send.menu_opened(&wire, &facts(), Some(&states()), &[], 3000);
    assert_eq!(
        status_runs(&again),
        0,
        "a second tailscale status run started while the first still runs"
    );
}

#[test]
fn r2_g_a_status_answer_after_a_backend_death_still_lands() {
    let (mut send, wire, i18n) = (ProviderSend::default(), Fake::default(), english());
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.backend_dead();
    send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    assert_eq!(
        send.peers_for(Role::Tailscale).len(),
        1,
        "the host's status answer was dropped"
    );
}

// A missing send script starts no status run, so a stale pick reads the gone peer, never "checking".
#[test]
fn a_missing_send_script_never_reads_as_checking() {
    let mut send = ProviderSend::default();
    let facts = Facts {
        send_command: String::new(),
        ..facts()
    };
    let opened = send.menu_opened(&Fake::default(), &facts, Some(&states()), &[], 0);
    assert_eq!(
        status_runs(&opened),
        0,
        "a missing send script starts no status run"
    );
    assert!(
        !send.taildrop_refreshing(),
        "a missing send script leaves no run in flight"
    );
    let i18n = english();
    assert_eq!(
        send.send_taildrop("nodekey:owned", &["/home/gm/a.txt".to_string()], &i18n),
        vec![Action::Say(
            "That Taildrop peer is no longer available.".into(),
            true
        )]
    );
}
