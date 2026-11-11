//! Plugin switches, disabled or failed rows, the gated-off menu and the Dropbox membership rule.
use super::*;

// r1-13: a plugin switch while the menu stays open re-derives the rows and asks the backend nothing.
#[test]
fn r1_13_a_plugin_switch_while_the_menu_stays_open_runs_nothing() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    let lines_before = wire.lines.borrow().len();
    let mut off = states();
    off[1].enabled = false;
    send.states_changed(Some(&off), &[]);
    assert!(
        !send.row_on(Role::LocalSend),
        "the LocalSend row leaves at once"
    );
    assert!(send.row_on(Role::Tailscale));
    assert_eq!(
        wire.lines.borrow().len(),
        lines_before,
        "a switch asks the backend nothing"
    );
    assert!(
        send.taildrop_refreshing(),
        "the run in flight is still the one that answers"
    );
}
#[test]
fn disabled_or_failed_plugins_lose_their_rows_in_menus_and_settings() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    send.menu_opened(&wire, &facts(), None, &[], 0);
    assert!(
        send.row_on(Role::LocalSend),
        "before the st2 join rows follow installed alone"
    );
    let mut off = states();
    off[1].enabled = false;
    send.menu_opened(&wire, &facts(), Some(&off), &[], 0);
    assert!(!send.row_on(Role::LocalSend));
    assert!(send.row_on(Role::Tailscale));
    let mut failed = states();
    failed[0].failed = true;
    send.menu_opened(&wire, &facts(), Some(&failed), &[], 0);
    assert!(!send.row_on(Role::Tailscale));
    send.menu_opened(
        &wire,
        &facts(),
        Some(&states()),
        &[Role::Dropbox.menu_id().to_string()],
        0,
    );
    assert!(
        !send.row_on(Role::Dropbox),
        "the Settings Menus switch hides the same row"
    );
}

#[test]
fn a_gated_off_row_does_zero_work_on_menu_open() {
    let mut send = ProviderSend::default();
    let wire = Fake::default();
    let mut off = states();
    for state in &mut off {
        state.enabled = false;
    }
    let actions = send.menu_opened(&wire, &facts(), Some(&off), &[], 0);
    assert!(wire.lines.borrow().is_empty());
    assert!(actions.is_empty());
    assert!(!send.localsend_checking());
    assert!(!send.taildrop_refreshing());
}
#[test]
fn dropbox_membership_is_the_backend_s_own_rule() {
    let send = ProviderSend::default();
    assert!(send.dropbox_member("/custom/Dropbox", "/custom/Dropbox/file.txt"));
    assert!(!send.dropbox_member("/custom/Dropbox", "/custom/Dropbox-old/file.txt"));
}

// r2-1: a plugin switched off after the menu was built never reaches the Taildrop spawn.
#[test]
fn r2_a_a_disabled_tailscale_plugin_never_spawns_a_send() {
    let (mut send, wire, i18n) = (ProviderSend::default(), Fake::default(), english());
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    assert!(send
        .taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n)
        .is_empty());
    let mut off = states();
    off[0].enabled = false;
    send.states_changed(Some(&off), &[]);
    assert!(!send.row_on(Role::Tailscale));
    let actions = send.send_taildrop("nodekey:owned", &["/home/gm/a.txt".to_string()], &i18n);
    assert_eq!(
        actions,
        vec![Action::Say(
            "That menu is out of date. Reopen it.".into(),
            true
        )],
        "a disabled Tailscale plugin still had a live peer"
    );
}

// r2-1: the row-off open drops the last open's peers, so neither the row nor a pick can use them.
#[test]
fn r2_b_an_uninstalled_tailscale_keeps_no_peer_to_send_to() {
    let (mut send, wire, i18n) = (ProviderSend::default(), Fake::default(), english());
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    assert_eq!(send.peers_for(Role::Tailscale).len(), 1);
    let mut gone = facts();
    gone.tailscale_command = String::new();
    send.menu_opened(&wire, &gone, Some(&states()), &[], 10_000);
    assert!(!send.row_on(Role::Tailscale));
    assert!(
        send.peers_for(Role::Tailscale).is_empty(),
        "the last open's peer still shows after tailscale left"
    );
    let actions = send.send_taildrop("nodekey:owned", &["/home/gm/a.txt".to_string()], &i18n);
    assert_eq!(
        actions,
        vec![Action::Say(
            "That menu is out of date. Reopen it.".into(),
            true
        )]
    );
}

// r2-1: a disabled LocalSend plugin writes no line to the backend and says the stale-menu sentence.
#[test]
fn r2_c_a_disabled_localsend_plugin_writes_nothing_to_the_backend() {
    let (mut send, wire, i18n) = (ProviderSend::default(), Fake::default(), english());
    let mut off = states();
    off[1].enabled = false;
    send.menu_opened(&wire, &facts(), Some(&off), &[], 0);
    assert!(wire.lines.borrow().is_empty());
    let actions = send.send_localsend(&wire, "Clean Lemon", &["/home/gm/a.txt".to_string()], &i18n);
    assert!(
        wire.lines.borrow().is_empty(),
        "a disabled LocalSend plugin queued {:?}",
        wire.lines.borrow()
    );
    assert_eq!(
        actions,
        vec![Action::Say(
            "That menu is out of date. Reopen it.".into(),
            true
        )]
    );
}

// r2-1: ShareLink rides the Dropbox provider, so a disabled Dropbox runs no sharelink process.
#[test]
fn r2_d_a_disabled_dropbox_plugin_runs_no_sharelink() {
    let (mut send, wire, i18n) = (ProviderSend::default(), Fake::default(), english());
    let mut off = states();
    off[2].enabled = false;
    send.menu_opened(&wire, &facts(), Some(&off), &[], 0);
    assert!(!send.row_on(Role::Dropbox));
    let actions = send.copy_share_link("/home/gm/file.txt", &i18n);
    assert_eq!(
        actions,
        vec![Action::Say(
            "That menu is out of date. Reopen it.".into(),
            true
        )]
    );
}

// r3-3: the r2-1 fix line clears a row's peers whenever the row is off at states_changed.
#[test]
fn r3_c_a_switch_that_hides_a_row_forgets_its_peers() {
    let (mut send, wire, i18n) = (ProviderSend::default(), Fake::default(), english());
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.line(
        r#"{"t":"localsendpeers","id":1,"peers":[{"name":"Clean Lemon","address":"192.168.21.23"}],"reason":""}"#,
        &i18n,
        0,
    );
    send.taildrop_done(FIRST_RUN, 0, &ready_status(), "", &i18n);
    assert_eq!(
        (
            send.peers_for(Role::LocalSend).len(),
            send.peers_for(Role::Tailscale).len()
        ),
        (1, 1)
    );
    let mut off = states();
    off[0].enabled = false;
    off[1].enabled = false;
    send.states_changed(Some(&off), &[]);
    let left = (
        send.peers_for(Role::LocalSend).len(),
        send.peers_for(Role::Tailscale).len(),
    );
    assert_eq!(
        left,
        (0, 0),
        "a switched-off row kept its peers (LocalSend, Tailscale) {left:?}"
    );
}

// r3-3: a LocalSend row switched off at menu open forgets its earlier peers too.
#[test]
fn r3_d_a_disabled_localsend_open_forgets_its_peers() {
    let (mut send, wire, i18n) = (ProviderSend::default(), Fake::default(), english());
    send.menu_opened(&wire, &facts(), Some(&states()), &[], 0);
    send.line(
        r#"{"t":"localsendpeers","id":1,"peers":[{"name":"Clean Lemon","address":"192.168.21.23"}],"reason":""}"#,
        &i18n,
        0,
    );
    assert_eq!(send.peers_for(Role::LocalSend).len(), 1);
    let mut off = states();
    off[1].enabled = false;
    send.menu_opened(&wire, &facts(), Some(&off), &[], 1000);
    assert!(
        send.peers_for(Role::LocalSend).is_empty(),
        "a disabled LocalSend open kept the last open's peers"
    );
}
