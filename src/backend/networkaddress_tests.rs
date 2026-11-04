use super::*;

#[test]
fn underscore_hosts_remain_valid_when_typed_and_reopened() {
    let address = parse("smb://file_server/share").expect("typed underscore host must remain valid");
    assert_eq!(address.host, "file_server");
    assert_eq!(address.uri, "smb://file_server/share");
    assert_eq!(address.path, "/share");
    assert_eq!(parse(&address.uri).unwrap(), address, "saved URI must reopen unchanged");
}

#[test]
fn nearby_avahi_underscore_host_produces_no_row() {
    let line = "=;eth0;IPv4;NAS;_smb._tcp;local;file_server.local;192.0.2.1;445;\"path=/share\"\n";
    let control = line.replace("file_server.local", "file-server.local");
    let rows = crate::backend::nearbyrows::avahi(&control, "_smb._tcp");
    assert_eq!(rows.len(), 1, "the same resolve line must work without the underscore");
    assert_eq!(rows[0].address.host, "file-server.local");
    eprintln!("F10 Avahi control: one row for {}", rows[0].address.host);
    assert!(crate::backend::nearbyrows::avahi(line, "_smb._tcp").is_empty());
}

#[test]
fn nearby_tailnet_underscore_host_produces_no_row() {
    let status = r#"{"BackendState":"Running","Peer":{"one":{"Online":true,"DNSName":"file_server.tail.ts.net.","HostName":"NAS","OS":"linux"}}}"#;
    let control = status.replace("file_server.tail.ts.net.", "file-server.tail.ts.net.");
    let rows = crate::backend::nearbyrows::tailscale(&control).unwrap();
    assert_eq!(rows.len(), 1, "the same peer must work without the underscore");
    assert_eq!(rows[0].address.host, "file-server.tail.ts.net");
    eprintln!("F10 tailnet control: one row for {}", rows[0].address.host);
    assert!(crate::backend::nearbyrows::tailscale(status).unwrap().is_empty());
}

#[test]
fn every_address_form_builds_the_saved_place_uri() {
    let cases = [
        ("smb://NAS.local/share", "smb://nas.local/share", "SMB", 445, "", "/share"),
        ("sftp://pi.local/home/pi", "sftp://pi.local/home/pi", "SFTP", 22, "", "/home/pi"),
        ("ftps://nas:990/files", "ftps://nas:990/files", "FTPS", 990, "", "/files"),
        ("dav://nas:80/docs", "dav://nas/docs", "WebDAV", 80, "", "/docs"),
        ("davs://nas:443/docs", "davs://nas/docs", "WebDAV", 443, "", "/docs"),
        ("nfs://nas:2049/export", "nfs://nas/export", "NFS", 2049, "", "/export"),
        ("gm@minipc:/srv/media", "sftp://gm@minipc/srv/media", "SFTP", 22, "gm", "/srv/media"),
        ("gm@[2001:db8::1]:/srv/media", "sftp://gm@[2001:db8::1]/srv/media", "SFTP", 22, "gm", "/srv/media"),
        ("sftp://user@[::1]:2222/home", "sftp://user@[::1]:2222/home", "SFTP", 2222, "user", "/home"),
        (" SMB://NAS.local.:445/ ", "smb://nas.local/", "SMB", 445, "", "/"),
        ("sftp://gm@nas/a%20b/caf%C3%A9/", "sftp://gm@nas/a%20b/caf%C3%A9", "SFTP", 22, "gm", "/a b/café"),
        ("gm@nas:/100% ready#file?", "sftp://gm@nas/100%25%20ready%23file%3F", "SFTP", 22, "gm", "/100% ready#file?"),
        ("ftps://nas:21", "ftps://nas/", "FTPS", 21, "", "/"),
        ("dav://nas:443", "dav://nas:443/", "WebDAV", 443, "", "/"),
        ("davs://nas:80", "davs://nas:80/", "WebDAV", 80, "", "/"),
        ("dav://nas/a%2fb/%2e%2e", "dav://nas/a%2Fb/%2E%2E", "WebDAV", 80, "", "/a/b/.."),
        ("dav://nas/a%2f/", "dav://nas/a%2F", "WebDAV", 80, "", "/a/"),
    ];
    for (input, uri, protocol, port, user, path) in cases {
        let address = parse(input).unwrap_or_else(|error| panic!("{input}: {error}"));
        assert_eq!((&*address.uri, &*address.protocol, address.port, &*address.user, &*address.path), (uri, protocol, port, user, path), "{input}");
        assert_eq!(parse(&address.uri).unwrap(), address, "normalized URI remains stable: {input}");
    }
    let domain = parse("smb://WORK;First%20Last@nas/share").unwrap();
    assert_eq!(domain.domain, "WORK");
    assert_eq!(domain.user, "First Last");
    assert_eq!(domain.uri, "smb://WORK;First%20Last@nas/share");
}

#[test]
fn malformed_addresses_never_reach_the_mount_flow() {
    for input in [
        "", " ", "/home/gm", "nas", "https://nas/a", "ftp://nas/a", "smb:///share",
        "sftp://", "sftp://-option/", "sftp://host-/", "sftp://host..local/", "sftp://a b/",
        "sftp://host:0/", "sftp://host:65536/", "sftp://host:-1/", "sftp://host:22x/", "sftp://host:/",
        "sftp://[not-ip]/", "sftp://[::1", "sftp://[::1]suffix/a", "sftp://::1/a", "sftp://[::1]:/a",
        "sftp://@host/a", "sftp://a@b@host/a", "sftp://user:password@host/a", "sftp://user%3Apassword@host/a",
        "nfs://user@nas/export", "smb://;user@nas/", "smb://DOMAIN;@nas/",
        "sftp://host/a%", "sftp://host/%GG", "sftp://host/%00", "sftp://host/%0A", "sftp://host/%FF",
        "sftp://host/path?query", "sftp://host/path#fragment", "sftp://host/a\nb", "user@host:relative",
        "\nsftp://host/", "sftp://host/\t",
        "@host:/absolute", "user@host:", "user@[::1]/path", "user@[::1]:relative",
    ] {
        assert_eq!(parse(input), Err(INVALID), "{input:?}");
    }
    assert_eq!(parse(&format!("sftp://host/{}", "a".repeat(8192))), Err(INVALID));
}

#[test]
fn wire_reply_escapes_paths_and_echoes_request_identity() {
    let reply = crate::jsondoc::parse(&answer(7, "gm@host:/a\"b")).unwrap();
    assert_eq!(reply.get("id").and_then(crate::jsondoc::Json::as_f64), Some(7.0));
    assert_eq!(reply.get("uri").and_then(crate::jsondoc::Json::as_str), Some("sftp://gm@host/a%22b"));
    assert_eq!(reply.get("path").and_then(crate::jsondoc::Json::as_str), Some("/a\"b"));
    let refused = crate::jsondoc::parse(&answer(8, "sftp://user:secret@host")).unwrap();
    assert_eq!(refused.get("ok").and_then(crate::jsondoc::Json::as_bool), Some(false));
    assert!(!answer(8, "sftp://user:secret@host").contains("secret"));
}
