//! A reopen in the same session reuses the password the server took, and a refusal forgets it.
use super::mount::{Effect, Mounts, Phase, Request};
use crate::g2::device_rail::{process::Outcome, secret::Secret};

const URI: &str = "smb://omarchy@127.0.0.1/secret";

fn request(password: &[u8]) -> Request {
    Request {
        id: 0,
        uri: URI.into(),
        label: "secret".into(),
        origin: 3,
        mounted: false,
        authenticated: false,
        password: Secret::from_bytes(password),
    }
}

fn auth_stdin(effects: &[Effect]) -> Option<Vec<u8>> {
    match effects {
        [Effect::Run {
            phase: Phase::Auth,
            job,
            ..
        }] => job.stdin.as_ref().map(|input| input.expose().to_vec()),
        _ => None,
    }
}

fn needs_password(effects: &[Effect]) -> bool {
    effects.iter().any(|e| {
        matches!(
            e,
            Effect::Retry {
                error: "network.password_needed",
                ..
            }
        )
    })
}

fn mounted(mounts: &mut Mounts) {
    let generation = mounts.generation;
    mounts.exited(generation, Phase::Auth, &Outcome::code(0), 1);
    let effects = mounts.exited(
        generation,
        Phase::Info,
        &Outcome {
            code: 0,
            stdout: "local path: /fixture/share\n".into(),
            stderr: String::new(),
        },
        2,
    );
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::EnsureBridge { .. })));
    assert!(!mounts.busy());
}

fn opened() -> Mounts {
    let mut mounts = Mounts {
        auth_binary: "/fixture/auth".into(),
        ..Mounts::default()
    };
    let effects = mounts.open_share(request(b"flea-fixture"), 0);
    assert_eq!(
        auth_stdin(&effects).as_deref(),
        Some(&b"flea-fixture\n"[..])
    );
    mounted(&mut mounts);
    mounts
}

#[test]
fn uish_misc_reopen_reuses_the_session_password() {
    let mut mounts = opened();
    let effects = mounts.open_share(request(b""), 10);
    assert_eq!(
        auth_stdin(&effects).as_deref(),
        Some(&b"flea-fixture\n"[..]),
        "the reopen sends the remembered password, never asks again"
    );
    mounted(&mut mounts);
}

#[test]
fn uish_misc_refused_password_is_forgotten() {
    let mut mounts = opened();
    mounts.open_share(request(b""), 10);
    let effects = mounts.exited(mounts.generation, Phase::Auth, &Outcome::code(77), 11);
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Retry {
            error: "network.password_refused",
            ..
        }
    )));
    assert!(needs_password(&mounts.open_share(request(b""), 12)));
}

#[test]
fn uish_misc_session_passwords_end_with_the_session() {
    let mut mounts = opened();
    mounts.stop_all();
    assert!(needs_password(&mounts.open_share(request(b""), 10)));
}

#[test]
fn uish_misc_another_address_gets_no_remembered_password() {
    let mut mounts = opened();
    let mut other = request(b"");
    other.uri = "smb://omarchy@127.0.0.1/other".into();
    assert!(needs_password(&mounts.open_share(other, 10)));
}

#[test]
fn uish_misc_a_password_the_server_never_took_is_not_kept() {
    let mut mounts = opened();
    mounts.open_share(request(b"mistyped"), 10);
    let effects = mounts.exited(mounts.generation, Phase::Auth, &Outcome::code(1), 11);
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::Retry {
            error: "network.failed",
            ..
        }
    )));
    mounts.open_share(request(b"mistyped"), 20);
    let due = mounts.deadline().unwrap();
    assert!(mounts
        .tick(due)
        .iter()
        .any(|e| matches!(e, Effect::Kill(_))));
    let effects = mounts.open_share(request(b""), due + 1);
    assert_eq!(
        auth_stdin(&effects).as_deref(),
        Some(&b"flea-fixture\n"[..]),
        "a failed or timed out attempt leaves the accepted password in place"
    );
}

#[test]
fn uish_misc_a_timed_out_first_attempt_is_not_replayed() {
    let mut mounts = Mounts {
        auth_binary: "/fixture/auth".into(),
        ..Mounts::default()
    };
    mounts.open_share(request(b"mistyped"), 0);
    let due = mounts.deadline().unwrap();
    assert!(mounts
        .tick(due)
        .iter()
        .any(|e| matches!(e, Effect::Kill(_))));
    assert!(needs_password(&mounts.open_share(request(b""), due + 1)));
}
