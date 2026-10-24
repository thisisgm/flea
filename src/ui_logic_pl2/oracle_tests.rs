//! Exact network.js label, authority and port cases exercise the shared production primitives.
use super::{
    form::{Form, Protocol},
    mounts, protocols, *,
};

#[test]
fn pl2_oracle_network_mount_labels_and_authorities() {
    for (label, uri, expected) in [
        ("isos en 192.168.1.10", "smb://192.168.1.10/isos/", "isos"),
        ("isos on 192.168.1.10", "smb://192.168.1.10/isos/", "isos"),
        ("isos sur nas", "smb://nas/isos/", "isos"),
        (
            "backup on tuesday",
            "smb://nas/backup%20on%20tuesday/",
            "backup on tuesday",
        ),
        (
            "backup on tuesday on nas",
            "smb://nas/backup%20on%20tuesday/",
            "backup on tuesday",
        ),
        ("media library", "dav://nas.local/media/", "media library"),
        ("nas", "smb://nas/", "nas"),
        ("nas on nas", "smb://nas/nas/", "nas"),
        ("isosnas", "smb://nas/isos/", "isosnas"),
        ("on nas", "smb://nas/on/", "on nas"),
        (
            "isos en fe80::1",
            "smb://[fe80::1]/isos/",
            "isos en fe80::1",
        ),
    ] {
        let output = format!("Mount(0): {label} -> {uri}\n");
        assert_eq!(mounts::parse_mounts(&output)[0].label, expected, "{uri}");
    }
    assert!(mounts::parse_mounts("Mount(0): Pixel 7 -> mtp://Google_Pixel_7_1A2B/\n").is_empty());
    for (uri, expected) in [
        ("sftp://u@h:22/inbox@2026", "h"),
        ("smb://[fe80::1]:445/isos/", "[fe80::1]"),
        ("smb://WORK;gm@nas/isos/", "nas"),
        ("file:///home/gm", ""),
        ("", ""),
    ] {
        assert_eq!(protocols::host_of(uri), expected);
    }
    assert_eq!(local_path("uri: smb://nas/\nlocal path: /run/user/1000/gvfs/smb-share:server=nas,share=isos\nunix mount: gvfsd-fuse\n"), "/run/user/1000/gvfs/smb-share:server=nas,share=isos");
    for output in [
        "uri: smb://nas/\ntype: directory\n",
        "",
        "ruta local: /other\n",
    ] {
        assert_eq!(local_path(output), "");
    }
}

#[test]
fn pl2_oracle_network_default_and_distinct_ports() {
    for (input, expected) in [
        ("smb://nas:445/isos/", "smb://nas/isos"),
        ("smb://nas:4450/isos/", "smb://nas:4450/isos"),
        ("smb://nas:445", "smb://nas/"),
        ("sftp://u@h:22/inbox@2026", "sftp://u@h/inbox@2026"),
    ] {
        assert_eq!(mounts::normalize(input), expected);
    }
    for (input, expected) in [
        ("smb://[fe80::445]/isos/", "smb://[fe80::445]/isos/"),
        ("smb://[fe80::1]:445/isos/", "smb://[fe80::1]/isos/"),
        ("smb://h:445/x", "smb://h/x"),
        ("sftp://h:22/x", "sftp://h/x"),
        ("ftp://h:21/x", "ftp://h/x"),
        ("ftps://h:21/x", "ftps://h/x"),
        ("dav://h:80/x", "dav://h/x"),
        ("davs://h:443/x", "davs://h/x"),
        ("dav://h:443/x", "dav://h:443/x"),
        ("davs://h:80/x", "davs://h:80/x"),
        ("ftps://h:990/x", "ftps://h:990/x"),
        ("sftp://h:445/x", "sftp://h:445/x"),
        ("/home/gm", "/home/gm"),
    ] {
        assert_eq!(protocols::strip_default_port(input), expected);
    }
    assert_eq!(
        mounts::normalize("nfs://h:2049/export"),
        mounts::normalize("nfs://h/export")
    );
    let mut f = Form {
        protocol: Protocol::WebDav,
        host: "nas.local".into(),
        path: "/dav".into(),
        ..Form::default()
    };
    for (tls, uri) in [
        (false, "dav://nas.local/dav"),
        (true, "davs://nas.local/dav"),
    ] {
        f.tls = tls;
        f.port = f.protocol.port(tls).to_string();
        assert_eq!(f.uri().as_deref(), Some(uri));
        assert_eq!(
            mounts::normalize(&f.uri().unwrap()),
            mounts::normalize(&format!("{uri}/"))
        );
    }
    assert_ne!(
        mounts::normalize("dav://nas.local:443/dav"),
        mounts::normalize("dav://nas.local/dav/")
    );
    for uri in [
        "smb://tom@nas.test/data",
        "ftp://tom@nas.test/",
        "davs://tom@nas.test/d",
        "file:///home/tom",
    ] {
        assert!(!keyless(uri));
    }
    for uri in ["sftp://tom@nas.test/home", "sftp://nas.test/"] {
        assert!(keyless(uri));
    }
    assert!(credentialed("sftp://tom@nas.test/home"));
    assert!(!credentialed("sftp://nas.test/home"));
    assert!(!credentialed("file:///home/tom"));
}

#[test]
fn pl2_oracle_remove_keeps_every_unrelated_byte() {
    let body = "file:///home/gm/Downloads Downloads\nsmb://nas:445/isos NAS isos\nsmb://other/data Other\n";
    let remove = super::places::remove_bookmark;
    assert_eq!(
        remove(body, "smb://nas/isos/"),
        "file:///home/gm/Downloads Downloads\nsmb://other/data Other\n"
    );
    assert_eq!(remove(body, "smb://gone/x"), body);
    assert_eq!(
        remove("smb://nas/isos2 Other\n", "smb://nas/isos"),
        "smb://nas/isos2 Other\n"
    );
    assert_eq!(remove("  smb://nas/isos NAS\n", "smb://nas/isos"), "");
    assert_eq!(remove(body, ""), body);
    assert_eq!(remove("", "smb://nas/isos"), "");
}
