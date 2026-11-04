//! The full form relay preserves empty-password mounts and successful bare-server credentials.
use super::{
    dialog::Dialog,
    mount::{Effect, Mounts, Phase, Request},
    relay,
};
use crate::g2::pl1::{process::Outcome, secret::Secret};
use flea::ui_logic_pl2::form::Protocol;

#[test]
fn pl2_empty_form_password_keeps_anonymous_and_nfs_mounts_available() {
    for protocol in [Protocol::Smb, Protocol::Sftp, Protocol::Nfs] {
        let mut d = Dialog::default();
        d.open();
        d.form.pick(protocol);
        d.form.host = "host".into();
        let request = relay::submitted(d.submit().pop().unwrap()).unwrap();
        assert!(!request.authenticated);
        let mut mounts = Mounts::default();
        assert!(matches!(
            mounts.open_share(request, 0).as_slice(),
            [Effect::Run {
                phase: Phase::Mount,
                ..
            }]
        ));
    }
}

#[test]
fn pl2_child_share_inherits_the_browsed_servers_session_password_and_origin() {
    let mut mounts = Mounts {
        auth_binary: "/fixture/auth".into(),
        ..Mounts::default()
    };
    let effects = mounts.open_share(
        Request {
            id: 0,
            uri: "smb://user@host/".into(),
            label: "Host".into(),
            origin: 17,
            mounted: false,
            authenticated: true,
            password: Secret::from_bytes(b"fixture-password"),
        },
        0,
    );
    assert!(
        matches!(&effects[0], Effect::Run { job, .. } if job.argv == ["timeout", "--", "35", "/fixture/auth", "smb://user@host/"] && job.stdin.as_ref().unwrap().expose() == b"fixture-password\n")
    );
    assert_eq!(mounts.deadline(), Some(35_000));
    let generation = mounts.generation;
    mounts.exited(generation, Phase::Auth, &Outcome::code(0), 1);
    mounts.exited(generation, Phase::Info, &Outcome::code(0), 2);
    let effects = mounts.exited(
        generation,
        Phase::Shares,
        &Outcome {
            code: 0,
            stdout: "media\n".into(),
            stderr: String::new(),
        },
        3,
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::Shares { origin: 17, .. }]
    ));
    let effects = mounts.open_child_share("smb://user@host/media/".into(), "media".into(), 17, 4);
    assert!(
        matches!(&effects[0], Effect::Run { phase: Phase::Auth, job, .. } if job.stdin.as_ref().unwrap().expose() == b"fixture-password\n" && !job.argv.iter().any(|a| a.contains("password")))
    );
    mounts.exited(mounts.generation, Phase::Auth, &Outcome::code(0), 5);
    let effects = mounts.exited(
        mounts.generation,
        Phase::Info,
        &Outcome {
            code: 0,
            stdout: "local path: /fixture/media\n".into(),
            stderr: String::new(),
        },
        6,
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::EnsureBridge { origin: 17, .. }]
    ));
}

#[test]
fn pl2_saved_password_addresses_and_malformed_ports_never_reach_argv_or_echo() {
    for uri in [
        "sftp://user:fixture-password@host/share",
        "sftp://user%3Afixture-password@host/share",
        "smb://host:65536/share",
    ] {
        for id in [0, 7] {
            let mut mounts = Mounts::default();
            let effects = mounts.open_share(
                Request {
                    id,
                    uri: uri.into(),
                    label: "Saved".into(),
                    origin: 17,
                    mounted: false,
                    authenticated: false,
                    password: Secret::new(),
                },
                0,
            );
            assert!(!effects.iter().any(|e| matches!(e, Effect::Run { .. })));
            assert!(!format!("{effects:?}").contains("fixture-password"));
            assert!(!mounts.busy());
        }
    }
}
