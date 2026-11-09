//! An SMB server that mounts for a guest and lists nothing hands the card its sign-in fields, never an error.
use super::{
    dialog::{Dialog, Focus},
    mount::{self, Mounts, Phase, Request, GUEST_EMPTY},
    result::ResultState,
};
use crate::g2::device_rail::{process::Outcome, secret::Secret};
use flea::ui_logic_network_shares::{form::Protocol, networkaddress};

fn request(id: u64, uri: &str) -> Request {
    Request {
        id,
        uri: uri.into(),
        label: "nas".into(),
        origin: 7,
        mounted: true,
        authenticated: false,
        password: Secret::new(),
    }
}

/// Runs a bare-root open through Info to the share listing, which answers with `listed`.
fn listing(request: Request, listed: Outcome) -> Vec<mount::Effect> {
    let mut m = Mounts::default();
    m.open_share(request, 0);
    let generation = m.generation;
    assert!(m
        .exited(generation, Phase::Info, &Outcome::code(1), 10)
        .iter()
        .any(|e| matches!(
            e,
            mount::Effect::Run {
                phase: Phase::Shares,
                ..
            }
        )));
    let effects = m.exited(generation, Phase::Shares, &listed, 11);
    assert!(!m.busy());
    effects
}

#[test]
fn a_guest_empty_listing_asks_for_a_name_on_the_card_and_says_nothing_on_the_pane() {
    let effects = listing(request(5, "smb://nas.local/"), Outcome::default());
    assert!(matches!(
        effects.as_slice(),
        [mount::Effect::Completed {
            id: 5,
            error: Some(GUEST_EMPTY),
            connect_failed: false,
            ..
        }]
    ));
    assert_eq!(
        ResultState::effect(&effects[0]),
        Some(ResultState::MissingCredential)
    );
}

#[test]
fn a_rail_open_of_a_guest_empty_server_reopens_the_card_as_an_invitation() {
    let effects = listing(request(0, "smb://nas.local/"), Outcome::default());
    assert!(matches!(
        effects.as_slice(),
        [mount::Effect::Retry {
            error: GUEST_EMPTY,
            connect_failed: false,
            origin: 7,
            ..
        }]
    ));
}

#[test]
fn a_named_or_failed_listing_keeps_the_0_4_0_refusal() {
    // A listing that failed, or an empty one under a name, is the server's real answer.
    let failed = listing(request(5, "smb://nas.local/"), Outcome::code(2));
    assert!(matches!(
        failed.as_slice(),
        [
            mount::Effect::Completed {
                error: Some("network.no_folder"),
                ..
            },
            mount::Effect::Say { .. }
        ]
    ));
    let named = listing(request(5, "smb://gm@nas.local/"), Outcome::default());
    assert!(matches!(
        named.as_slice(),
        [
            mount::Effect::Completed {
                error: Some("network.no_folder"),
                ..
            },
            mount::Effect::Say { .. }
        ]
    ));
    let mut signed = request(5, "smb://nas.local/");
    signed.authenticated = true;
    let signed = listing(signed, Outcome::default());
    assert!(matches!(
        signed.as_slice(),
        [
            mount::Effect::Completed {
                error: Some("network.no_folder"),
                ..
            },
            mount::Effect::Say { .. }
        ]
    ));
    let mut keyed = request(5, "smb://nas.local/");
    keyed.password = Secret::from_bytes(b"pw");
    let keyed = listing(keyed, Outcome::default());
    assert!(matches!(
        keyed.as_slice(),
        [
            mount::Effect::Completed {
                error: Some("network.no_folder"),
                ..
            },
            mount::Effect::Say { .. }
        ]
    ));
}

#[test]
fn the_card_takes_the_invitation_with_the_caret_in_username() {
    let mut d = Dialog::default();
    d.open();
    d.form.host = "nas.local".into();
    let effects = d.submit();
    assert_eq!(effects.len(), 1);
    let (id, uri) = (d.request, d.pending_uri.clone());
    assert!(d
        .mount_finished(id, &uri, Err(GUEST_EMPTY), false)
        .is_empty());
    assert_eq!(d.status, Some(GUEST_EMPTY));
    assert!(!d.failed_connect && d.opened);
    assert_eq!(d.focus, Focus::User);
    assert!(d.ring().contains(&Focus::User) && d.ring().contains(&Focus::Password));
}

#[test]
fn a_picked_host_fills_protocol_host_port_and_path_and_waits_on_connect() {
    let mut d = Dialog::default();
    d.open();
    let address = networkaddress::parse("sftp://pi.local:2222/srv").unwrap();
    d.suggest(&address);
    assert_eq!(d.form.protocol, Protocol::Sftp);
    assert_eq!(
        (
            d.form.host.as_str(),
            d.form.port.as_str(),
            d.form.path.as_str()
        ),
        ("pi.local", "2222", "srv")
    );
    assert_eq!(d.focus, Focus::Connect);
    assert!(!d.connecting, "nothing connects until Connect");
    // A closed card takes no suggestion, so a late pick cannot reopen or refill it.
    let mut closed = Dialog::default();
    closed.suggest(&address);
    assert!(!closed.opened && closed.form.host.is_empty());
}
