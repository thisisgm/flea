//! Credential ownership and dispatch validation cover the review's trust boundaries.
use super::{
    dialog::{Dialog, Effect as DialogEffect},
    menus,
    mount::{Effect, Mounts, Phase, Request},
    relay,
};
use crate::g2::pl1::{process::Outcome, secret::Secret};
use flea::{
    ui_logic_pl1::{Kind, RailEntry},
    ui_logic_pl2::form::Protocol,
};

fn request(uri: &str, origin: u64) -> Request {
    Request {
        id: 0,
        uri: uri.into(),
        label: "Fixture".into(),
        origin,
        mounted: false,
        authenticated: true,
        password: Secret::from_bytes(b"synthetic-secret"),
    }
}
fn browser(m: &mut Mounts, uri: &str, origin: u64, password: &[u8]) {
    let mut r = request(uri, origin);
    r.password = Secret::from_bytes(password);
    m.open_share(r, 0);
    let g = m.generation;
    m.exited(g, Phase::Auth, &Outcome::code(0), 1);
    m.exited(g, Phase::Info, &Outcome::code(0), 2);
    m.exited(
        g,
        Phase::Shares,
        &Outcome {
            code: 0,
            stdout: "media\n a%20b \n".into(),
            stderr: String::new(),
        },
        3,
    );
}
fn sent(effects: &[Effect]) -> &[u8] {
    effects
        .iter()
        .find_map(|e| match e {
            Effect::Run {
                job,
                phase: Phase::Auth,
                ..
            } => job.stdin.as_ref().map(Secret::expose),
            _ => None,
        })
        .expect("a valid child uses its own browser credential")
}

#[test]
fn fix1_browser_credentials_are_scoped_consumed_and_validate_server_user_and_share() {
    let mut m = Mounts::default();
    browser(&mut m, "smb://WORK;user@first:1445/", 1, b"synthetic-first");
    browser(&mut m, "smb://user@second/", 2, b"synthetic-second");
    for (uri, origin) in [
        ("smb://WORK;user@first:1445/media/", 2),
        ("smb://WORK;other@first:1445/media/", 1),
        ("smb://OTHER;user@first:1445/media/", 1),
        ("smb://WORK;user@first/media/", 1),
        ("smb://WORK;user@first:1445/unlisted/", 1),
        ("smb://WORK;user@first:1445/media%2Fchild/", 1),
    ] {
        let e = m.open_child_share(uri.into(), "media".into(), origin, 4);
        assert!(!e.iter().any(|e| matches!(e, Effect::Run { .. })), "{uri}");
        assert!(m.has_credentials(1) && m.has_credentials(2));
    }
    let e = m.open_child_share(
        "smb://WORK;user@first:1445/media/".into(),
        "media".into(),
        1,
        4,
    );
    assert_eq!(sent(&e), b"synthetic-first\n");
    assert!(!m.browsers.contains_key(&1));
    m.cancel_origin(1);
    assert!(!m.has_credentials(1));
    assert!(m.has_credentials(2));
    let e = m.open_child_share("smb://user@second/media/".into(), "media".into(), 2, 5);
    assert_eq!(sent(&e), b"synthetic-second\n");
}

#[test]
fn fix1_operation_and_browser_secrets_end_with_their_owner() {
    for end in [
        "complete", "fail", "cancel", "timeout", "retire", "unmount", "stop",
    ] {
        let mut m = Mounts::default();
        m.open_share(request("smb://user@host/share", 7), 0);
        assert!(m.has_credentials(7));
        let g = m.generation;
        match end {
            "complete" => {
                m.exited(g, Phase::Auth, &Outcome::code(0), 1);
                m.exited(
                    g,
                    Phase::Info,
                    &Outcome {
                        code: 0,
                        stdout: "local path: /fixture/share\n".into(),
                        stderr: String::new(),
                    },
                    2,
                );
                assert!(!m.has_credentials(7), "a detached bridge needs no password");
                m.bridge_ready(g, "/fixture/share", 3);
            }
            "fail" => {
                let e = m.exited(g, Phase::Auth, &Outcome::code(1), 1);
                assert!(e
                    .iter()
                    .all(|e| !matches!(e, Effect::Retry { password, .. } if !password.is_empty())));
            }
            "cancel" => {
                m.cancel(0);
            }
            "timeout" => {
                m.tick(35_000);
            }
            "retire" => {
                m.cancel_origin(7);
            }
            "unmount" => {
                m.retire_location("smb://user@host/share");
            }
            "stop" => m.stop_all(),
            _ => unreachable!(),
        }
        assert!(!m.has_credentials(7), "{end}");
    }
    for end in ["retire", "unmount", "replace", "stop"] {
        let mut m = Mounts::default();
        browser(&mut m, "smb://user@host/", 7, b"synthetic-secret");
        assert!(m.has_credentials(7));
        match end {
            "retire" => {
                m.cancel_origin(7);
            }
            "unmount" => {
                m.retire_location("smb://user@host/media");
            }
            "replace" => {
                let mut r = request("smb://other/", 7);
                r.password.clear();
                r.authenticated = false;
                m.open_share(r, 4);
            }
            "stop" => m.stop_all(),
            _ => unreachable!(),
        }
        assert!(!m.has_credentials(7), "{end}");
    }
}

#[test]
fn fix1_secrets_stay_in_the_live_draft_and_nfs_cannot_authenticate() {
    let mut d = Dialog::default();
    d.open();
    d.form.host = "host".into();
    d.password.push_str("synthetic-secret");
    let e = d.submit().pop().unwrap();
    assert_eq!(d.password.expose(), b"synthetic-secret");
    assert!(
        matches!(&e, DialogEffect::Mount { password, .. } if password.expose() == b"synthetic-secret")
    );
    for protocol in [Protocol::Smb, Protocol::Nfs] {
        let mut d = Dialog::default();
        d.open();
        d.form.host = "host".into();
        d.password.push_str("synthetic-secret");
        d.pick(if protocol == Protocol::Nfs { 4 } else { 0 });
        if protocol == Protocol::Smb {
            d.form.port = "invalid".into();
        }
        let effects = d.submit();
        assert!(d.password.is_empty());
        if protocol == Protocol::Smb {
            assert!(effects.is_empty());
            continue;
        }
        let r = relay::submitted(effects.into_iter().last().unwrap()).unwrap();
        assert!(!r.authenticated && r.password.is_empty());
        let mut m = Mounts::default();
        assert!(
            matches!(m.open_share(r, 0).as_slice(), [Effect::Run { phase: Phase::Mount, job, .. }] if job.argv == ["gio", "mount", "--", "nfs://host/"] && job.stdin.is_none())
        );
    }
    let fake = DialogEffect::Mount {
        id: 1,
        uri: "nfs://host/export".into(),
        label: "NFS".into(),
        origin: 7,
        password: Secret::from_bytes(b"synthetic-secret"),
    };
    let r = relay::submitted(fake).unwrap();
    assert!(!r.authenticated && r.password.is_empty());
    let mut m = Mounts::default();
    let e = m.open_share(request("nfs://host/export", 7), 0);
    assert!(
        matches!(e.as_slice(), [Effect::Run { phase: Phase::Mount, job, .. }] if job.stdin.is_none())
    );
    assert!(!m.has_credentials(7));
}

#[test]
fn fix1_unsupported_or_secret_identities_cannot_open_or_unmount_even_while_busy() {
    for uri in [
        "https://user:synthetic-token@host/share",
        "https://host/share",
        "other://host/share",
        "sftp://user%3Asynthetic-token@host/share",
        "smb://host:65536/share",
    ] {
        for busy in [false, true] {
            let mut m = Mounts::default();
            if busy {
                m.open_share(request("smb://user@valid/share", 1), 0);
            }
            let mut r = request(uri, 7);
            r.id = 1;
            r.mounted = true;
            let e = m.open_share(r, 1);
            assert!(!e.iter().any(|e| matches!(e, Effect::Run { .. })));
            assert!(!format!("{e:?}").contains("synthetic-token"));
        }
        for fstype in ["", "fuse.sshfs"] {
            let entry = RailEntry {
                uri: uri.into(),
                path: "/fixture/mount".into(),
                fstype: fstype.into(),
                mounted: true,
                kind: Kind::Share,
                ..RailEntry::default()
            };
            let job = menus::unmount(&entry, "");
            if fstype.is_empty() {
                assert!(job.is_none());
            } else {
                // FIX2: a FUSE row releases by its own path, so the listed uri never reaches argv.
                let argv = job.unwrap().argv;
                assert_eq!(argv, ["fusermount3", "-u", "--", "/fixture/mount"]);
                assert!(!argv.join(" ").contains("synthetic-token"));
            }
        }
    }
    let mut entry = RailEntry {
        uri: "sftp://user@HOST:22/docs".into(),
        mounted: true,
        kind: Kind::Share,
        ..RailEntry::default()
    };
    let job = menus::unmount(&entry, "").unwrap();
    assert_eq!(
        job.argv,
        ["gio", "mount", "-u", "--", "sftp://user@host/docs"]
    );
    entry.uri = "/fixture/kernel".into();
    entry.path = entry.uri.clone();
    entry.fstype = "nfs".into();
    assert_eq!(
        menus::unmount(&entry, "").unwrap().argv,
        ["gio", "mount", "-u", "--", "/fixture/kernel"]
    );
    entry.fstype = "fuse.sshfs".into();
    assert_eq!(
        menus::unmount(&entry, "").unwrap().argv,
        ["fusermount3", "-u", "--", "/fixture/kernel"]
    );
    entry.path = "--bad".into();
    assert!(menus::unmount(&entry, "").is_none());
}
