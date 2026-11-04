use super::{
    dialog::Focus,
    mount::{self, Mounts, Phase, Request},
    routing::{address, AddressRoute},
};
use crate::g2::pl1::{process::Outcome, secret::Secret};
use flea::ui_logic_pl2::{bridge, mounts::Place, places::Aliases, rail};

#[test]
fn pl2_path_addresses_share_the_rail_opener_without_saving() {
    for text in [
        "smb://nas/share",
        "sftp://u@host:2222/a%2fb",
        "ftps://host/",
        "dav://host/",
        "davs://host/",
        "nfs://host/export",
        "user@[::1]:/path",
    ] {
        let AddressRoute::Network(r) = address(text, 42) else {
            panic!("valid network route refused");
        };
        assert_eq!((r.id, r.origin), (0, 42));
        assert!(r.password.is_empty());
        assert!(!r.authenticated && !r.mounted);
    }
    for text in [
        "smb://user:password@nas/share",
        "smb://nas:0/",
        "user@host:relative",
        "https://host/",
    ] {
        assert!(matches!(
            address(text, 0),
            AddressRoute::Invalid("network.invalid_address")
        ));
    }
    assert!(matches!(address("/home/gm", 0), AddressRoute::Local));
    assert!(matches!(address("../a", 0), AddressRoute::Local));
}

#[test]
fn pl2_alias_delivery_and_saved_relabels_stay_independent() {
    let mut aliases = Aliases::default();
    let saved = [Place {
        uri: "davs://first/path/".into(),
        label: "Private files".into(),
    }];
    aliases.remember("/second", "Second", "davs://second/path", &saved);
    aliases.remember("/first", "Generated first", "davs://first/path", &saved);
    assert_eq!(
        aliases.at("/first/Documents").unwrap().label,
        "Private files"
    );
    assert_eq!(aliases.at("/second").unwrap().label, "Second");
    assert!(aliases.at("/first-other").is_none());
    assert!(!aliases.relabel(&saved));
    let saved = [Place {
        uri: "davs://first/path".into(),
        label: "Renamed".into(),
    }];
    assert!(aliases.relabel(&saved));
    assert!(!aliases.relabel(&saved));
    assert_eq!(aliases.at("/first").unwrap().label, "Renamed");
}

#[test]
fn pl2_network_rows_use_mountinfo_without_touching_mountpoints() {
    let gio = "Mount(0): photos -> file:///mnt/photos\nMount(1): USB -> file:///run/media/gm/USB\nMount(2): isos en nas -> smb://nas/isos/\n";
    let info = "30 1 0:45 / /mnt/photos rw - nfs4 nas:/photos rw\n31 1 0:46 / /mnt/fuse\\040drive rw - fuse.rclone remote: rw\n32 1 8:2 / /run/media/gm/USB rw - ext4 /dev/sdb1 rw\n33 1 0:47 / /mnt/idle rw - autofs systemd-1 rw\n34 1 0:48 / /mnt/live rw - autofs systemd-1 rw\n35 34 0:49 / /mnt/live rw - nfs4 nas:/live rw\n36 1 0:50 / /run/user/1000/gvfs rw - fuse.gvfsd-fuse gvfsd-fuse rw\n";
    let rows = rail::rebuild(gio, &[], info);
    assert_eq!(rows.len(), 4);
    assert_eq!(
        (&*rows[1].label, &*rows[1].uri, &*rows[1].fstype),
        ("photos", "file:///mnt/photos", "nfs4")
    );
    assert_eq!(rows[2].path, "/mnt/fuse drive");
    assert_eq!(rows[3].path, "/mnt/live");
    assert!(rows[1..]
        .iter()
        .all(|r| r.editable == Some(false) && r.mounted));
    let saved = [Place {
        uri: "smb://nas:445/isos".into(),
        label: "My NAS".into(),
    }];
    assert_eq!(rail::rebuild(gio, &saved, "")[0].label, "My NAS");
    assert!(rail::covers("sftp://u@host/", "sftp://u@host/path"));
    assert!(!rail::covers("smb://host/", "smb://host/share"));
    assert!(!rail::covers("sftp://u@host/", "sftp://v@host/path"));
}

#[test]
fn pl2_bridge_paths_are_scoped_and_daemon_argv_is_literal() {
    let root = bridge::bridge_dir("/run/user/1000");
    assert_eq!(root, bridge::bridge_dir(""));
    assert!(bridge::needs_bridge(&root, &root));
    assert!(bridge::needs_bridge(
        &(root.clone() + "/smb-share:server=nas,share=x"),
        &root
    ));
    assert!(!bridge::needs_bridge("/run/user/1000/doc/1/file", &root));
    assert!(!bridge::needs_bridge("/run/user/1000/gvfs-other", &root));
    assert_eq!(
        bridge::fuse_argv("", &root),
        ["/usr/lib/gvfsd-fuse", &root, "-f"]
    );
}

#[test]
fn pl2_delayed_bridge_reply_keeps_the_original_subject() {
    let mut m = Mounts::default();
    let request = |host: &str, origin| Request {
        id: 0,
        uri: format!("davs://{host}/path"),
        label: host.into(),
        origin,
        mounted: true,
        authenticated: false,
        password: Secret::new(),
    };
    m.open_share(request("first", 1), 0);
    let first = m.generation;
    let out = Outcome {
        stdout: "local path: /first\n".into(),
        ..Outcome::default()
    };
    m.exited(first, Phase::Info, &out, 1);
    assert!(!m.busy());
    m.open_share(request("second", 2), 2);
    let second = m.generation;
    let out = Outcome {
        stdout: "local path: /second\n".into(),
        ..Outcome::default()
    };
    m.exited(second, Phase::Info, &out, 3);
    assert!(
        matches!(m.bridge_ready(first, "/first", 4).as_slice(), [mount::Effect::Open { origin: 1, label, .. }] if label == "first")
    );
    assert!(
        matches!(m.bridge_ready(second, "/second", 5).as_slice(), [mount::Effect::Open { origin: 2, label, .. }] if label == "second")
    );
    assert!(m.bridge_ready(first, "/first", 6).is_empty());
}

#[test]
fn pl2_secret_composition_replaces_marked_text_without_plaintext_copies() {
    let mut secret = Secret::from_bytes("a猫z".as_bytes());
    let mut entry = crate::path::entry::Entry::default();
    crate::g2::pl1::private_entry::begin(&mut entry, &secret);
    assert!(crate::g2::pl1::private_entry::replace(
        &mut entry,
        &mut secret,
        Some(1..2),
        "日",
        false
    ));
    assert_eq!(entry.ime_selection().range, 2..2);
    assert!(secret.expose() == "a日z".as_bytes());
    assert!(crate::g2::pl1::private_entry::replace(
        &mut entry,
        &mut secret,
        Some(1..2),
        "日本",
        false
    ));
    assert_eq!(entry.ime_selection().range, 3..3);
    assert!(secret.expose() == "a日本z".as_bytes());
    assert!(!format!("{secret:?}").contains("日本"));
    secret.clear();
    assert_eq!(super::secret_edit::units(&secret), 0);
}

#[test]
fn pl2_protocol_focus_hides_no_live_field() {
    let mut dialog = super::dialog::Dialog::default();
    dialog.open();
    dialog.focus = Focus::Domain;
    dialog.pick(1);
    assert_eq!(dialog.focus, Focus::User);
    for _ in 0..dialog.ring().len() {
        dialog.step(false);
        assert!(dialog.ring().contains(&dialog.focus));
    }
}

#[test]
fn pl2_dialog_submission_uses_explicit_auth_and_preserves_origin() {
    let mut d = super::dialog::Dialog::default();
    d.origin = 17;
    d.open();
    d.form.protocol = flea::ui_logic_pl2::form::Protocol::Sftp;
    d.form.host = "host".into();
    d.form.port = "22".into();
    d.form.user = "user".into();
    d.password.push_str("fixture-password");
    let effect = d.submit().pop().unwrap();
    let request = super::relay::submitted(effect).unwrap();
    assert!(request.authenticated);
    assert_eq!(request.origin, 17);
    let mut mounts = Mounts::default();
    assert!(matches!(
        mounts.open_share(request, 0).as_slice(),
        [mount::Effect::Run {
            phase: Phase::Auth,
            ..
        }]
    ));
    assert!(flea::ui_logic_pl2::form::Form::saved_address("ftp://user@host:2121/path").is_ok());
    assert!(matches!(
        address("ftp://user@host/path", 0),
        AddressRoute::Invalid(_)
    ));
}

#[test]
fn pl2_failed_and_cancelled_credentials_never_enter_the_session_cache() {
    for timed in [false, true] {
        let mut mounts = Mounts::default();
        let mut request = super::mount::Request {
            id: 1,
            uri: "smb://user@host/share".into(),
            label: "Fixture".into(),
            origin: 1,
            mounted: false,
            authenticated: true,
            password: Secret::new(),
        };
        request.password.push_str("fixture-password");
        mounts.open_share(request, 0);
        if timed {
            mounts.tick(35_001);
        } else {
            mounts.exited(
                mounts.generation,
                super::mount::Phase::Auth,
                &crate::g2::pl1::process::Outcome::code(1),
                1,
            );
        }
        assert!(!mounts.has_credentials(1));
    }
}
