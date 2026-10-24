use super::{bridge::*, form::*, *};

#[test]
fn pl2_headings_follow_the_catalog_language() {
    assert_eq!(heading::heading("isim", "tr"), ("İSİM".into(), true));
    assert_eq!(
        heading::heading("κεφαλίδα", "el"),
        ("ΚΕΦΑΛΙΔΑ".into(), true)
    );
    assert_eq!(heading::heading("接続", "ja"), ("接続".into(), false));
    assert_eq!(heading::heading("المضيف", "ar"), ("المضيف".into(), false));
}

#[test]
fn pl2_protocols_and_uri_recipes() {
    let mut form = Form {
        host: "nas.local".into(),
        path: "docs and photos/#?".into(),
        ..Form::default()
    };
    for (protocol, expected) in PROTOCOLS.into_iter().zip([
        "smb://nas.local/docs%20and%20photos/%23%3F",
        "sftp://nas.local/docs%20and%20photos/%23%3F",
        "ftps://nas.local/docs%20and%20photos/%23%3F",
        "davs://nas.local/docs%20and%20photos/%23%3F",
        "nfs://nas.local/docs%20and%20photos/%23%3F",
    ]) {
        form.pick(protocol);
        assert_eq!(form.uri().as_deref(), Some(expected));
    }
    for (protocol, port) in PROTOCOLS.into_iter().zip([445, 22, 21, 443, 2049]) {
        form.tls = true;
        form.pick(protocol);
        assert_eq!(form.port, port.to_string());
    }
    form.pick(Protocol::WebDav);
    form.toggle_tls();
    assert_eq!(form.port, "80");
    assert_eq!(
        form.uri().unwrap(),
        "dav://nas.local/docs%20and%20photos/%23%3F"
    );
    form.port = "443".into();
    assert!(form.uri().unwrap().starts_with("dav://nas.local:443/"));
    form.pick(Protocol::Ftps);
    assert!(form.uri().unwrap().starts_with("ftp://"));
}

#[test]
fn pl2_form_labels_domains_and_negative_ports() {
    let mut f = Form {
        host: "NAS".into(),
        user: "gm".into(),
        domain: "WORK".into(),
        path: "media".into(),
        ..Form::default()
    };
    assert_eq!(f.uri().as_deref(), Some("smb://WORK;gm@nas/media"));
    assert_eq!(f.name(), "media");
    f.label = " NAS media ".into();
    assert_eq!(f.name(), "NAS media");
    for port in ["0", "65536", "-1", "12x", "22.0"] {
        f.port = port.into();
        assert!(f.uri().is_none(), "{port}");
    }
    f.port.clear();
    assert!(f.uri().is_some());
    f.host = "nas/path".into();
    assert!(f.uri().is_none());
}

#[test]
fn pl2_suggestion_keeps_label_credentials_and_exact_address() {
    let mut f = Form {
        label: "My place".into(),
        user: "user".into(),
        domain: "WORK".into(),
        ..Form::default()
    };
    let a = networkaddress::parse("davs://server:8443/a%20b").unwrap();
    f.suggest(&a);
    assert_eq!(
        (&*f.label, &*f.user, &*f.domain),
        ("My place", "user", "WORK")
    );
    assert_eq!(
        (f.protocol, f.tls, &*f.host, &*f.port, &*f.path),
        (Protocol::WebDav, true, "server", "8443", "a b")
    );
}

#[test]
fn pl2_smb_identity_repair_is_scoped_by_host_share_and_user() {
    let base = "/run/user/1000/gvfs/smb-share:server=nas,share=test";
    assert!(!smb::matches("smb://dan@nas/test", base));
    assert!(smb::matches(
        "smb://DOMAIN;dan@NAS/Test",
        &format!("{base},user=dan")
    ));
    assert!(smb::matches("smb://dan@nas/test", "/unknown-shape"));
    let paths = vec![format!("{base},user=dan"), format!("{base},user=gm")];
    assert_eq!(
        smb::resolve("smb://dan@nas/test", &paths, "gm").path,
        paths[0]
    );
    assert_eq!(smb::resolve("smb://nas/test", &paths, "gm").path, paths[1]);
    assert_eq!(
        smb::resolve("smb://nas/test", &paths, "other").users,
        ["dan", "gm"]
    );
    assert!(smb::resolve("smb://third@nas/test", &paths, "gm")
        .users
        .is_empty());
    assert!(smb::resolve("smb://other/test", &paths, "gm")
        .path
        .is_empty());
}

#[test]
fn pl2_bridge_served_directory_and_file_never_start_a_daemon() {
    let mut b = Bridge::default();
    assert_eq!(b.ensure(0), Action::CheckDirectory);
    assert_eq!(b.ensure(1), Action::Busy);
    assert_eq!(b.checked(true, 2), Some(Action::Ready(true)));
    b.ensure(3);
    assert_eq!(b.checked(false, 4), Some(Action::CheckFile));
    assert_eq!(b.checked(true, 5), Some(Action::Ready(false)));
    assert_eq!(b.starts, 0);
    assert_eq!(b.deadline(), None);
}

#[test]
fn pl2_bridge_has_one_start_one_deadline_and_no_late_failure() {
    for phase in [
        Phase::Directory,
        Phase::File,
        Phase::Waiting,
        Phase::Classify,
    ] {
        let mut b = Bridge::default();
        b.ensure(0);
        if phase != Phase::Directory {
            b.checked(false, 1);
        }
        if matches!(phase, Phase::Waiting | Phase::Classify) {
            assert_eq!(b.checked(false, 2), Some(Action::Start));
        }
        if phase == Phase::Classify {
            b.checked(true, 3);
        }
        assert_eq!(b.tick(ENSURE_MS), [Action::Failed]);
        assert!(b.tick(ENSURE_MS + 1).is_empty());
        assert_eq!(b.checked(true, ENSURE_MS + 2), None);
    }
    let mut b = Bridge::default();
    b.ensure(0);
    b.checked(false, 1);
    b.checked(false, 2);
    assert!(!b.tick(249).contains(&Action::Show));
    assert!(b.tick(250).contains(&Action::Show));
    assert!(!b.tick(300).contains(&Action::Show));
    assert_eq!(b.checked(true, 301), Some(Action::Classify));
    assert_eq!(b.checked(false, 302), Some(Action::Ready(false)));
    assert_eq!(b.starts, 1);
    b.ensure(400);
    b.cancel();
    assert!(b.tick(20_000).is_empty());
}

#[test]
fn pl2_shared_mount_rules_keep_authority_and_labels() {
    assert_eq!(
        local_path("uri: smb://nas/\nlocal path: /run/user/1000/gvfs/x\n"),
        "/run/user/1000/gvfs/x"
    );
    assert_eq!(local_path("ruta local: /other"), "");
    assert!(credentialed("sftp://u@h/inbox"));
    assert!(!credentialed("sftp://h/inbox@2026"));
    assert!(keyless("SFTP://u@h/"));
    assert!(bare_root("smb://nas/"));
    assert!(!bare_root("smb://nas/share"));
    assert_eq!(
        mounts::normalize("sftp://u@h:22/inbox@2026/"),
        "sftp://u@h/inbox@2026"
    );
    for label in ["isos en nas", "isos on nas", "isos sur nas"] {
        assert_eq!(protocols::share_name(label, "smb://nas/isos/"), "isos");
    }
}
