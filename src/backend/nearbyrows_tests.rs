use super::*;

#[test]
fn avahi_decodes_names_txt_paths_and_every_service() {
    for (service, scheme) in SERVICES.into_iter().zip(["smb", "sftp", "dav", "davs", "nfs", "nfs"]) {
        let line = format!(r#"=;eth0;IPv4;NAS\032office\059one;{service};local;NAS.local;192.0.2.1;8043;"note=ignored;value" "path=/docs\032and\032photos""#);
        let rows = avahi(&line, service);
        assert_eq!(rows.len(), 1, "{line}");
        assert_eq!(rows[0].name, "NAS office;one");
        assert_eq!(rows[0].address.scheme, scheme);
        assert_eq!(rows[0].address.host, "nas.local");
        assert_eq!(rows[0].address.port, 8043);
        assert_eq!(rows[0].address.path, "/docs and photos");
        assert_eq!(rows[0].address.uri, format!("{scheme}://nas.local:8043/docs%20and%20photos"));
    }
    let unicode = avahi(r#"=;eth0;IPv6;caf\195\169;_smb._tcp;local;nas.local;2001:db8::1;445;"path=/a\"b""#, "_smb._tcp");
    assert_eq!(unicode[0].name, "café");
    assert_eq!(unicode[0].address.path, "/a\"b");
}

#[test]
fn avahi_ignores_unresolved_foreign_and_hostile_rows() {
    for line in [
        "+;eth0;IPv4;NAS;_smb._tcp;local", "-;eth0;IPv4;NAS;_smb._tcp;local",
        "=;eth0;IPv4;NAS;_ssh._tcp;local;nas.local;192.0.2.1;22;",
        "=;eth0;IPv4;NAS;_smb._tcp;local;-option;192.0.2.1;445;",
        "=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;0;",
        "=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;65536;",
        r#"=;eth0;IPv4;NAS\000;_smb._tcp;local;nas.local;192.0.2.1;445;"#,
        r#"=;eth0;IPv4;NAS\999;_smb._tcp;local;nas.local;192.0.2.1;445;"#,
        r#"=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;445;"path=/\000""#,
        r#"=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;445;"path=/unterminated"#,
    ] {
        assert!(avahi(line, "_smb._tcp").is_empty(), "{line}");
    }
    assert!(avahi(&"x".repeat(9000), "_smb._tcp").is_empty());
}

#[test]
fn tailnet_uses_online_magicdns_peers_and_windows_smb_without_taildrop_gates() {
    let status = r#"{"BackendState":"Running","Peer":{
        "a":{"Online":true,"OS":"linux","HostName":"minipc","DNSName":"minipc.tail.ts.net."},
        "b":{"Online":true,"OS":"windows","HostName":"gm-desktop","DNSName":"gm-desktop.tail.ts.net."},
        "c":{"Online":false,"DNSName":"offline.tail.ts.net."},
        "d":{"Online":true,"DNSName":"exit.mullvad.ts.net."},
        "e":{"Online":true,"HostName":"ip-only","TailscaleIPs":["100.64.0.1"]},
        "f":{"Online":true,"DNSName":"--help"},
        "g":{"Online":true,"HostName":"localhost","DNSName":"pi.tail.ts.net."}
    }}"#;
    let rows = tailscale(status).unwrap();
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].address.uri, "sftp://minipc.tail.ts.net/");
    assert_eq!(rows[1].address.uri, "smb://gm-desktop.tail.ts.net/");
    assert_eq!(rows[2].name, "pi");
    assert!(rows.iter().all(|row| row.source == "tailscale"));
    assert!(tailscale(&status.replace("Running", "NeedsLogin")).unwrap().is_empty());
    assert!(tailscale("not json").is_err());
}

#[test]
fn duplicate_interfaces_collapse_but_distinct_protocols_and_paths_stay() {
    let line = "=;eth0;IPv4;NAS;_smb._tcp;local;NAS.local.;192.0.2.1;445;\"path=/one\"";
    let mut rows = avahi(line, "_smb._tcp");
    rows.extend(avahi(&line.replace("eth0", "wlan0").replace("IPv4", "IPv6"), "_smb._tcp"));
    rows.extend(avahi(&line.replace("/one", "/two"), "_smb._tcp"));
    rows.extend(avahi(&line.replace("_smb._tcp", "_sftp-ssh._tcp").replace(";445;", ";22;"), "_sftp-ssh._tcp"));
    assert_eq!(deduplicate(rows).len(), 3);
    let line = "=;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;445;";
    assert_eq!(avahi(line, "_smb._tcp")[0].address.path, "/");
    assert_eq!(avahi(&format!("{}\n", line).repeat(500), "_smb._tcp").len(), MAX_ROWS);
}

#[test]
fn reviewer_tailscale_wide_object_time_is_bounded() {
    let mut input = String::from("{\"BackendState\":\"Running\",\"Peer\":{");
    for index in 0..90_000 {
        if index != 0 { input.push(','); }
        use std::fmt::Write;
        write!(input, "\"{index:05}\":{{}}").unwrap();
    }
    input.push_str("}}");
    assert!(input.len() < 1024 * 1024);
    let started = std::time::Instant::now();
    assert!(tailscale(&input).unwrap().is_empty());
    let elapsed = started.elapsed();
    eprintln!("REVIEW wide Tailscale object: {} bytes, {elapsed:?}", input.len());
    assert!(elapsed < std::time::Duration::from_secs(3), "Tailscale parsing exceeded the whole discovery budget: {elapsed:?}");
}

#[test]
fn reviewer_unrelated_binary_txt_does_not_hide_a_service() {
    let line = r#"=;eth0;IPv4;NAS;_webdav._tcp;local;nas.local;192.0.2.1;80;"vendor=\000" "path=/docs""#;
    assert_eq!(avahi(line, "_webdav._tcp").len(), 1);
}
