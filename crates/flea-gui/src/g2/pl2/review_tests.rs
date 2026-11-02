use super::{
    dialog::{Dialog, Effect as DialogEffect},
    mount::{Effect, Mounts, Phase, Request},
    relay,
};
use crate::g2::pl1::{process::Outcome, secret::Secret};
use flea::ui_logic_pl2::{browser::Browser, form::Form, networkaddress};

fn request(uri: &str) -> Request {
    Request {
        id: 1,
        uri: uri.into(),
        label: "Fixture".into(),
        origin: 7,
        mounted: false,
        authenticated: false,
        password: Secret::new(),
    }
}
fn success(m: &mut Mounts) {
    let generation = m.generation;
    m.exited(generation, Phase::Auth, &Outcome::code(0), 1);
    m.exited(
        generation,
        Phase::Info,
        &Outcome {
            code: 0,
            stdout: "local path: /fixture/served\n".into(),
            stderr: String::new(),
        },
        2,
    );
    m.bridge_ready(generation, "/fixture/served", 3);
}

#[test]
fn review_password_retained_for_retry_and_cleared_when_closed() {
    let mut d = Dialog::default();
    d.open();
    d.form.host = "host".into();
    d.form.user = "user".into();
    d.password.push_str("synthetic-secret");
    d.submit();
    let uri = d.pending_uri.clone();
    d.mount_finished(d.request, &uri, Err("network.password_refused"), true);
    assert!(
        d.password.expose() == b"synthetic-secret",
        "connection error lost the live Retry draft"
    );
    assert!(!d.reveal);
    d.close();
    assert!(d.password.is_empty());
}

#[test]
fn review_password_cleared_when_origin_retires() {
    let mut m = Mounts::default();
    let mut r = request("smb://user@host/share");
    r.authenticated = true;
    r.password.push_str("synthetic-secret");
    m.open_share(r, 0);
    success(&mut m);
    m.cancel_origin(7);
    assert!(
        !m.has_credentials(7),
        "retired origin retained cached password"
    );
}

#[test]
fn review_unsupported_saved_uri_cannot_put_credentials_on_argv() {
    let mut m = Mounts::default();
    let mut r = request("https://user:synthetic-token@host/share");
    r.mounted = true;
    let effects = m.open_share(r, 0);
    assert!(
        !effects.iter().any(|e| matches!(e, Effect::Run {job, ..}
        if job.argv.iter().any(|arg| arg.contains("synthetic-token")))),
        "unsupported saved URI sends token in gio argv"
    );
}

#[test]
fn review_smb_repair_preserves_refused_cause() {
    let mut m = Mounts::default();
    m.open_share(request("smb://host/share"), 0);
    let generation = m.generation;
    m.exited(generation, Phase::Mount, &Outcome::code(1), 1);
    let path = "/run/user/1000/gvfs/smb-share:server=other,share=wrong";
    m.exited(
        generation,
        Phase::Info,
        &Outcome {
            code: 0,
            stdout: format!("local path: {path}\n"),
            stderr: String::new(),
        },
        2,
    );
    m.peeked(generation, path, &[], true, "user", 3);
    let effects = m.peeked(
        generation,
        "/run/user/1000/gvfs",
        &["smb-share:server=other,share=wrong".into()],
        false,
        "user",
        4,
    );
    assert!(
        effects.iter().any(|e| matches!(
            e,
            Effect::Completed {
                error: Some("network.refused"),
                ..
            }
        )),
        "SMB repair replaced refusal with no-folder: {effects:?}"
    );
}

#[test]
fn review_repaired_mount_completes_with_exact_submitted_uri() {
    let mut d = Dialog::default();
    d.open();
    d.form.host = "host".into();
    let mut m = Mounts::default();
    let r = relay::submitted(d.submit().pop().unwrap()).unwrap();
    let submitted = d.pending_uri.clone();
    m.open_share(r, 0);
    let generation = m.generation;
    m.exited(generation, Phase::Mount, &Outcome::code(0), 1);
    let effects = m.exited(
        generation,
        Phase::Info,
        &Outcome {
            code: 0,
            stdout: "local path: /fixture/served\n".into(),
            stderr: String::new(),
        },
        2,
    );
    let completed_uri = effects
        .iter()
        .find_map(|e| match e {
            Effect::Completed { uri, .. } => Some(uri),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        completed_uri, &submitted,
        "completion URI does not match dialog identity"
    );
}

#[test]
fn review_loaded_encoded_path_keeps_resource_identity() {
    let address = networkaddress::parse("sftp://user@host/a%2Fb").unwrap();
    let form = Form::load(&address, "Saved");
    assert_eq!(
        form.uri().as_deref(),
        Some(address.uri.as_str()),
        "loading saved URI changes encoded separator"
    );
}

#[test]
fn review_percent_in_share_name_is_literal() {
    let mut browser = Browser::default();
    browser.show("smb://host/".into(), "Host".into(), vec!["a%20b".into()], 7);
    let (uri, _, _) = browser.activate().unwrap();
    let mut m = Mounts::default();
    m.open_share(request("smb://host/"), 0);
    let generation = m.generation;
    m.exited(generation, Phase::Mount, &Outcome::code(0), 1);
    m.exited(generation, Phase::Info, &Outcome::code(0), 2);
    m.exited(
        generation,
        Phase::Shares,
        &Outcome {
            code: 0,
            stdout: "a%20b\n".into(),
            stderr: String::new(),
        },
        3,
    );
    let effects = m.open_child_share(uri, "a%20b".into(), 7, 0);
    let actual = effects
        .iter()
        .find_map(|e| match e {
            Effect::Run { job, .. } => job.argv.last(),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        actual, "smb://host/a%2520b",
        "raw share name interpreted as an escape"
    );
}

#[test]
fn review_nfs_does_not_submit_hidden_password() {
    let mut d = Dialog::default();
    d.open();
    d.form.host = "host".into();
    d.password.push_str("synthetic-secret");
    d.pick(4);
    let effect = d.submit().pop().unwrap();
    let DialogEffect::Mount { password, .. } = effect else {
        panic!("missing mount")
    };
    assert!(
        password.is_empty(),
        "NFS submission carried hidden SMB password"
    );
}

#[test]
fn review_two_panes_cannot_cross_server_passwords() {
    let mut m = Mounts::default();
    for (host, origin, password) in [
        ("first", 1, "synthetic-first"),
        ("second", 2, "synthetic-second"),
    ] {
        let mut r = request(&format!("smb://user@{host}/"));
        r.id = 0;
        r.origin = origin;
        r.authenticated = true;
        r.password.push_str(password);
        m.open_share(r, 0);
        let generation = m.generation;
        m.exited(generation, Phase::Auth, &Outcome::code(0), 1);
        m.exited(generation, Phase::Info, &Outcome::code(0), 2);
        m.exited(
            generation,
            Phase::Shares,
            &Outcome {
                code: 0,
                stdout: "media\n".into(),
                stderr: String::new(),
            },
            3,
        );
    }
    let effects = m.open_child_share("smb://user@first/media/".into(), "media".into(), 1, 4);
    let sent_wrong_password = effects.iter().any(|e| {
        matches!(e, Effect::Run {job, ..}
        if job.stdin.as_ref().is_some_and(|secret| secret.expose() == b"synthetic-second\n"))
    });
    assert!(
        !sent_wrong_password,
        "second server password sent to first server after another pane browsed it"
    );
}
