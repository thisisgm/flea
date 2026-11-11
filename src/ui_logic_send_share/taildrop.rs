//! Taildrop peer eligibility and status, ported from ui/js/Taildrop.js for every front end.
//! Sample status input is `tailscale status --json`; only the fields read here are named.
use crate::i18n::I18n;
use crate::jsondoc::{self, Json};

// One eligible send target: the map key is the id, the label is the owner's own device name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub label: String,
    pub address: String,
}

// What status() names for logic; the reason stays diagnostic data, never a translated word.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub peers: Vec<Peer>,
    pub reason: String,
    pub code: String,
}

// The file-sharing capability the account must carry before any peer can send.
const FILE_SHARING: &str = "https://tailscale.com/cap/file-sharing";

// Sample input: {"Self":{"UserID":1},"Peer":{"nodekey:a":{"HostName":"laptop","Online":true,"TaildropTarget":1,"UserID":1}}}.
pub fn parse_peers(raw: &str) -> Vec<Peer> {
    let text = raw.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let data = match jsondoc::parse(text) {
        Ok(data) => data,
        Err(_) => return Vec::new(),
    };
    if !matches!(data, Json::Obj(_)) {
        return Vec::new();
    }
    let own = id_string(data.get("Self").and_then(|s| s.get("UserID")).unwrap_or(&Json::Null));
    let mut out = Vec::new();
    if let Some(peers) = data.get("Peer").and_then(Json::as_object) {
        for (id, peer) in peers {
            if peer.get("Online").and_then(Json::as_bool) != Some(true) {
                continue;
            }
            if is_mullvad(peer) {
                continue;
            }
            if !is_taildrop_target(peer, &own) {
                continue;
            }
            let address = peer_address(peer);
            if address.is_empty() {
                continue;
            }
            out.push(Peer { id: id.clone(), label: display_host_name(peer), address });
        }
    }
    // localeCompare puts DEVICE-A before laptop; a case-folded primary with a byte tiebreak agrees.
    out.sort_by(|a, b| {
        a.label.to_lowercase().cmp(&b.label.to_lowercase()).then_with(|| a.label.cmp(&b.label))
    });
    out
}

// Sample input: {"BackendState":"Running","Self":{"CapMap":{"https://tailscale.com/cap/file-sharing":[]}},"Peer":{}}; an older CLI lists the same URL in Self.Capabilities.
pub fn status(raw: &str, exit_code: i32, error: &str) -> Status {
    if exit_code != 0 {
        // The CLI's diagnostic rides the reason slot; the caller words an empty one through the catalog.
        return Status { peers: Vec::new(), reason: error.trim().into(), code: "statusFailed".into() };
    }
    let data = match jsondoc::parse(raw) {
        Ok(data) => data,
        Err(_) => return invalid(),
    };
    if !matches!(data, Json::Obj(_)) {
        return invalid();
    }
    if data.get("BackendState").and_then(Json::as_str) == Some("NeedsLogin") {
        return Status { peers: Vec::new(), reason: "signed out".into(), code: "signedOut".into() };
    }
    match data.get("BackendState").and_then(Json::as_str) {
        Some("Running") => {}
        other => {
            let reason = other.unwrap_or("unavailable").to_lowercase();
            return Status { peers: Vec::new(), reason, code: "stopped".into() };
        }
    }
    let own = data.get("Self").cloned().unwrap_or(Json::Null);
    let capabilities: Vec<&str> = own
        .get("Capabilities")
        .and_then(Json::as_array)
        .map(|items| items.iter().filter_map(Json::as_str).collect())
        .unwrap_or_default();
    let mapped = own.get("CapMap").and_then(|m| m.get(FILE_SHARING)).is_some();
    if !mapped && !capabilities.contains(&FILE_SHARING) {
        return Status { peers: Vec::new(), reason: "disabled for this account".into(), code: "disabled".into() };
    }
    let peers = parse_peers(raw);
    if peers.is_empty() {
        return Status { peers, reason: "no peers".into(), code: "noPeers".into() };
    }
    Status { peers, reason: String::new(), code: String::new() }
}

fn invalid() -> Status {
    Status { peers: Vec::new(), reason: "invalid status".into(), code: "invalid".into() }
}

// A specific TaildropTarget wins outright; 0 (unset) falls back to same-owner check.
pub fn is_taildrop_target(peer: &Json, own_user: &str) -> bool {
    match peer.get("TaildropTarget").and_then(Json::as_f64) {
        Some(target) if target != 0.0 => target == 1.0,
        _ => {
            let owner = id_string(peer.get("UserID").unwrap_or(&Json::Null));
            !owner.is_empty() && owner == own_user
        }
    }
}

fn clean_dns_name(name: &str) -> String {
    match name.strip_suffix('.') {
        Some(clean) => clean.into(),
        None => name.into(),
    }
}

fn short_dns_name(name: &str) -> String {
    let clean = clean_dns_name(name);
    if clean.is_empty() {
        return String::new();
    }
    clean.split('.').next().unwrap_or(&clean).into()
}

fn display_host_name(peer: &Json) -> String {
    let host = peer.get("HostName").and_then(Json::as_str).unwrap_or_default();
    if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return host.into();
    }
    let dns = peer.get("DNSName").and_then(Json::as_str).unwrap_or_default();
    let short = short_dns_name(dns);
    if short.is_empty() { host.into() } else { short }
}

// ui/js/Taildrop.js:83 ends its name chain on this word, so an address-only peer still draws a named row.
pub fn label_or_unknown(i18n: &I18n, label: &str) -> String {
    if label.is_empty() { i18n.tr("send.providers.unknown_peer") } else { label.into() }
}

// An exit-node relay peer, never a real send target; see the OEM's own isMullvadPeer.
pub fn is_mullvad(peer: &Json) -> bool {
    const SUFFIX: &str = ".mullvad.ts.net";
    let dns = clean_dns_name(peer.get("DNSName").and_then(Json::as_str).unwrap_or_default());
    let dns = dns.to_lowercase();
    let host = peer.get("HostName").and_then(Json::as_str).unwrap_or_default().to_lowercase();
    ends_with(&dns, SUFFIX) || ends_with(&host, SUFFIX)
}

fn ends_with(value: &str, suffix: &str) -> bool {
    value.len() > suffix.len() && value.ends_with(suffix)
}

// omarchy-tailscale-send resolves its own "$machine" argument the same three ways.
pub fn peer_address(peer: &Json) -> String {
    if let Some(dns) = peer.get("DNSName").and_then(Json::as_str) {
        if !dns.is_empty() {
            return clean_dns_name(dns);
        }
    }
    if let Some(host) = peer.get("HostName").and_then(Json::as_str) {
        if !host.is_empty() {
            return host.into();
        }
    }
    peer.get("TailscaleIPs")
        .and_then(Json::as_array)
        .map(|ips| {
            ips.iter()
                .filter_map(Json::as_str)
                .find(|ip| ip.starts_with("100."))
                .unwrap_or_default()
                .to_string()
        })
        .unwrap_or_default()
}

// 2^53, past which JS String(n) and an i64 print a whole number differently.
const JS_SAFE_LIMIT: f64 = 9007199254740992.0;

// JSON numbers stringify the way JS String() does for whole ids under 2^53; strings pass through.
fn id_string(value: &Json) -> String {
    match value {
        Json::Str(s) => s.clone(),
        Json::Num(n) => match n.parse::<i64>() {
            Ok(v) => v.to_string(),
            Err(_) => match n.parse::<f64>() {
                Ok(v) if v.fract() == 0.0 && v.abs() < JS_SAFE_LIMIT => format!("{}", v as i64),
                _ => n.clone(),
            },
        },
        _ => String::new(),
    }
}

pub fn by_id<'a>(peers: &'a [Peer], id: &str) -> Option<&'a Peer> {
    peers.iter().find(|peer| peer.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Paths;
    use std::path::PathBuf;

    fn english() -> I18n {
        let none = PathBuf::from("/nonexistent/flea-sd1");
        I18n::load(Paths { omarchy: none.clone(), language_file: none, catalogs: Vec::new() })
    }

    fn obj(text: &str) -> Json {
        jsondoc::parse(text).expect("a test object parses")
    }

    // The tests/js/taildrop.js Peer map, five branches; every name and id is synthetic.
    fn peer_map() -> &'static str {
        r#""Peer":{
"nodekey:offline":{"HostName":"mediabox","Online":false,"TaildropTarget":5,"UserID":1000000000000001},
"nodekey:owned":{"HostName":"laptop","Online":true,"TaildropTarget":1,"UserID":1000000000000001},
"nodekey:sameowner":{"HostName":"DEVICE-A","Online":true,"TaildropTarget":1,"UserID":1000000000000001},
"nodekey:otherowner":{"HostName":"OTHER-TENANT","Online":true,"TaildropTarget":9,"UserID":2000000000000002},
"nodekey:relay":{"HostName":"","DNSName":"foo.mullvad.ts.net.","Online":true,"TaildropTarget":1,"UserID":1000000000000001}}"#
    }

    fn status_fixture() -> String {
        format!(r#"{{"Self":{{"UserID":1000000000000001}},{}}}"#, peer_map())
    }

    // The same five-branch fixture taken to the ready shape, exactly as tests/js/taildrop.js builds it.
    fn ready_fixture() -> String {
        format!(
            r#"{{"BackendState":"Running","Self":{{"UserID":1000000000000001,"Capabilities":["https://tailscale.com/cap/file-sharing"]}},{}}}"#,
            peer_map()
        )
    }

    // The ready daemon with an empty Peer map: installed, capable, but no devices.
    fn ready_empty() -> String {
        r#"{"BackendState":"Running","Self":{"UserID":1000000000000001,
"Capabilities":["https://tailscale.com/cap/file-sharing"]},"Peer":{}}"#
            .into()
    }

    #[test]
    fn only_the_reachable_eligible_peers_survive_sorted() {
        let peers = parse_peers(&status_fixture());
        assert_eq!(peers.len(), 2);
        assert_eq!(format!("{},{}", peers[0].label, peers[1].label), "DEVICE-A,laptop");
        assert_eq!(peers[0].id, "nodekey:sameowner");
        let folded = r#"{"Self":{"UserID":1},"Peer":{
"nodekey:z":{"HostName":"Zeta","Online":true,"TaildropTarget":1},
"nodekey:a":{"HostName":"apple","Online":true,"TaildropTarget":1}}}"#;
        let labels: Vec<_> = parse_peers(folded).into_iter().map(|peer| peer.label).collect();
        assert_eq!(labels, ["apple", "Zeta"], "the order folds case, where byte order puts Zeta first");
    }

    #[test]
    fn an_unset_target_falls_back_to_the_numeric_owner() {
        let fixture = r#"{"Self":{"UserID":1000000000000001},"Peer":{
"nodekey:mine":{"HostName":"mine","Online":true,"TaildropTarget":0,"UserID":1000000000000001},
"nodekey:theirs":{"HostName":"theirs","Online":true,"TaildropTarget":0,"UserID":1000000000000002}}}"#;
        let ids: Vec<_> = parse_peers(fixture).into_iter().map(|peer| peer.id).collect();
        assert_eq!(ids, ["nodekey:mine"]);
    }

    #[test]
    fn empty_garbage_and_peerless_bodies_parse_to_no_peers() {
        assert!(parse_peers("").is_empty());
        assert!(parse_peers("not json").is_empty());
        assert!(parse_peers(r#"{"Self":{}}"#).is_empty());
        assert!(parse_peers("null").is_empty());
    }

    #[test]
    fn taildrop_target_branches_match_the_oem_model() {
        assert!(is_taildrop_target(&obj(r#"{"TaildropTarget":1}"#), "u"));
        assert!(!is_taildrop_target(&obj(r#"{"TaildropTarget":9}"#), "u"));
        assert!(is_taildrop_target(&obj(r#"{"TaildropTarget":0,"UserID":"u"}"#), "u"));
        assert!(!is_taildrop_target(&obj(r#"{"TaildropTarget":0,"UserID":"u"}"#), "other"));
    }

    #[test]
    fn send_address_prefers_dns_then_host_then_tailnet_ipv4() {
        assert_eq!(peer_address(&obj(r#"{"DNSName":"host.ts.net."}"#)), "host.ts.net");
        assert_eq!(peer_address(&obj(r#"{"HostName":"host"}"#)), "host");
        assert_eq!(peer_address(&obj(r#"{"TailscaleIPs":["100.1.2.3","fd7a::1"]}"#)), "100.1.2.3");
        assert_eq!(peer_address(&obj(r#"{}"#)), "");
    }

    #[test]
    fn a_localhost_hostname_falls_back_to_the_dns_name() {
        let peer = obj(r#"{"HostName":"localhost","DNSName":"example.ts.net."}"#);
        assert_eq!(display_host_name(&peer), "example");
    }

    // tests/js/taildrop.js:83: the name chain ends on the reserved word, resolved through the catalog.
    #[test]
    fn an_address_only_peer_keeps_its_diagnostic_label_and_the_callers_word_names_it() {
        let raw = r#"{"BackendState":"Running","Self":{"UserID":"u",
"Capabilities":["https://tailscale.com/cap/file-sharing"]},
"Peer":{"nodekey:ip":{"TailscaleIPs":["100.1.2.3"],"Online":true,"TaildropTarget":1,"UserID":"u"}}}"#;
        let peers = status(raw, 0, "").peers;
        assert_eq!(peers.len(), 1, "a peer with only an address is still a send target");
        assert_eq!(peers[0].label, "");
        assert_eq!(peers[0].address, "100.1.2.3");
        assert_eq!(label_or_unknown(&english(), &peers[0].label), "Unknown");
        assert_eq!(label_or_unknown(&english(), "laptop"), "laptop");
    }

    #[test]
    fn by_id_finds_the_right_peer_and_misses_cleanly() {
        let peers = parse_peers(&status_fixture());
        assert_eq!(by_id(&peers, "nodekey:owned").expect("owned").label, "laptop");
        assert!(by_id(&peers, "nope").is_none());
    }

    // The reasons and codes of tests/js/taildrop.js:66-74; the empty diagnostic's catalog word is the caller's.
    #[test]
    fn failed_malformed_signed_out_and_stopped_statuses_keep_code_and_reason() {
        assert_eq!(status("", 1, "Permission denied").reason, "Permission denied");
        assert_eq!(status("", 1, "Permission denied").code, "statusFailed");
        assert_eq!(status("", 1, "").reason, "");
        assert_eq!(status("", 1, "").code, "statusFailed");
        assert_eq!(status("not json", 0, "").reason, "invalid status");
        assert_eq!(status("not json", 0, "").code, "invalid");
        assert_eq!(status("7", 0, "").reason, "invalid status");
        assert_eq!(status("7", 0, "").code, "invalid");
        assert_eq!(status(r#"{"BackendState":"NeedsLogin"}"#, 0, "").reason, "signed out");
        assert_eq!(status(r#"{"BackendState":"NeedsLogin"}"#, 0, "").code, "signedOut");
        assert_eq!(status(r#"{"BackendState":"Stopped"}"#, 0, "").reason, "stopped");
        assert_eq!(status(r#"{"BackendState":"Stopped"}"#, 0, "").code, "stopped");
    }

    #[test]
    fn running_without_file_sharing_cannot_send() {
        let state = status(r#"{"BackendState":"Running","Self":{}}"#, 0, "");
        assert!(state.peers.is_empty());
        assert_eq!(state.reason, "disabled for this account");
        assert_eq!(state.code, "disabled");
    }

    // tests/js/taildrop.js:47-50 screens the five-branch fixture through status(), not a pre-filtered list.
    #[test]
    fn status_screens_the_five_branch_fixture_to_its_eligible_pair() {
        let ready = status(&ready_fixture(), 0, "");
        assert_eq!(ready.peers.len(), 2);
        assert_eq!(format!("{},{}", ready.peers[0].label, ready.peers[1].label), "DEVICE-A,laptop");
        assert_eq!(ready.reason, "");
        assert_eq!(ready.code, "");
        let empty = status(&ready_empty(), 0, "");
        assert!(empty.peers.is_empty());
        assert_eq!(empty.reason, "no peers");
        assert_eq!(empty.code, "noPeers");
    }
}
