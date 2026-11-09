use super::*;
use flea::ui_logic_mn1::Brand;

fn catalog() -> I18n {
    crate::g2::op3::tests::english()
}

// The line src/backend/nearby.rs writes for one LAN service and one tailnet peer.
const FOUND: &str = r#"{"t":"nearby","id":7,"state":"found","message":"","rows":[{"name":"backup","source":"mdns","uri":"smb://backup.local/","scheme":"smb","protocol":"smb","host":"backup.local","port":445,"user":"","domain":"","path":"/","tls":false},{"name":"east","source":"tailscale","uri":"sftp://east.tail.ts.net/","scheme":"sftp","protocol":"sftp","host":"east.tail.ts.net","port":22,"user":"","domain":"","path":"/","tls":false}],"errors":[]}"#;

#[test]
fn a_found_line_reads_each_host_with_its_address_and_source() {
    let reply = parse(FOUND).expect("nearby line");
    assert_eq!((reply.id, reply.state), (7, State::Found));
    let names: Vec<_> = reply
        .hosts
        .iter()
        .map(|host| (host.name.as_str(), host.tailnet))
        .collect();
    assert_eq!(names, [("backup", false), ("east", true)]);
    assert_eq!(reply.hosts[0].address.scheme, "smb");
    assert_eq!(reply.hosts[1].address.host, "east.tail.ts.net");
}

#[test]
fn this_networks_hosts_list_before_the_tailnets() {
    let line = r#"{"t":"nearby","id":1,"state":"found","message":"","rows":[{"name":"alpha","source":"tailscale","uri":"sftp://alpha.ts.net/"},{"name":"nas","source":"mdns","uri":"smb://nas.local/"},{"name":"zed","source":"mdns","uri":"sftp://zed.local/"}],"errors":[]}"#;
    let names: Vec<_> = parse(line)
        .unwrap()
        .hosts
        .into_iter()
        .map(|host| host.name)
        .collect();
    assert_eq!(names, ["nas", "zed", "alpha"]);
}

#[test]
fn looking_empty_and_foreign_lines_read_as_what_they_are() {
    let looking = r#"{"t":"nearby","id":2,"state":"looking","message":"","rows":[],"errors":[]}"#;
    assert_eq!(parse(looking).map(|r| r.state), Some(State::Looking));
    let empty = r#"{"t":"nearby","id":2,"state":"empty","message":"","rows":[],"errors":["tailscale: not found"]}"#;
    assert_eq!(parse(empty).map(|r| r.state), Some(State::Empty));
    // A found line whose rows all fail to parse is nothing found, never a menu of blanks.
    let broken = r#"{"t":"nearby","id":2,"state":"found","message":"","rows":[{"name":"x","source":"mdns","uri":"not a uri"},{"name":"","source":"mdns","uri":"smb://a.local/"}],"errors":[]}"#;
    let reply = parse(broken).unwrap();
    assert_eq!((reply.state, reply.hosts.len()), (State::Empty, 0));
    assert!(parse(r#"{"t":"networkaddress","id":2,"ok":false,"uri":"","error":"x"}"#).is_none());
    assert!(parse(r#"{"t":"rows","start":0,"rows":[{"n":"nearby"}]}"#).is_none());
}

#[test]
fn the_request_is_the_backend_wire_shape() {
    assert_eq!(request("start", 4), r#"{"c":"nearby","op":"start","id":4}"#);
    assert_eq!(request("stop", 4), r#"{"c":"nearby","op":"stop","id":4}"#);
}

#[test]
fn hosts_draw_the_nearby_rows_and_add_a_network_share_is_last() {
    let catalog = catalog();
    let hosts = parse(FOUND).unwrap().hosts;
    let rows = entries(State::Found, &hosts, &catalog);
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[0].label, Some(Text::Data("backup".into())));
    assert_eq!(rows[0].glyph, Some("server"));
    assert_eq!(rows[0].brand, None);
    assert_eq!(rows[0].hint, Some(Text::Data("SMB · backup.local".into())));
    assert_eq!(rows[1].brand, Some(Brand::Tailscale));
    assert_eq!(rows[1].hint, Some(Text::Data("SFTP · Tailscale".into())));
    assert!(rows[2].separator);
    assert_eq!(rows[3].action, ADD_SHARE);
    assert_eq!(rows[3].label, Some(Text::Key("network.add_share")));
    assert_eq!(rows[3].glyph, Some("plus"));
    assert_eq!(
        chosen(&rows[1].action, &hosts).map(|h| h.name.as_str()),
        Some("east")
    );
    assert!(chosen(ADD_SHARE, &hosts).is_none());
}

#[test]
fn looking_holds_one_busy_row_that_never_takes_the_cursor() {
    let rows = entries(State::Looking, &[], &catalog());
    assert_eq!(rows.len(), 3);
    assert!(rows[0].busy && !rows[0].steppable());
    assert_eq!(rows[0].label, Some(Text::Key("network.looking_hosts")));
    assert!(rows[1].separator);
    assert!(rows[2].steppable() && rows[2].action == ADD_SHARE);
    // Nothing found leaves the one row the plus always offered, with no break above it.
    let empty = entries(State::Empty, &[], &catalog());
    assert_eq!(empty.len(), 1);
    assert_eq!(empty[0].action, ADD_SHARE);
}

#[test]
fn every_protocol_names_the_card_chip_it_fills() {
    let catalog = catalog();
    for (uri, word) in [
        ("smb://h.local/", "SMB"),
        ("sftp://h.local/", "SFTP"),
        ("dav://h.local/", "WebDAV"),
        ("davs://h.local/", "WebDAV"),
        ("nfs://h.local/srv", "NFS"),
    ] {
        let host = Host {
            name: "h".into(),
            tailnet: false,
            address: networkaddress::parse(uri).unwrap(),
        };
        assert_eq!(meta(&host, &catalog), format!("{word} · h.local"), "{uri}");
    }
}
