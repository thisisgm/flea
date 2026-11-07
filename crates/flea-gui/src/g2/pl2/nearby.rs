//! The NETWORK plus's hosts menu: the backend's nearby discovery on the wire, and the rows the menu draws from its answer.
use flea::i18n::I18n;
use flea::jsondoc::{self, Json};
use flea::ui_logic_mn1::{Brand, Entry, Text};
use flea::ui_logic_pl2::networkaddress::{self, Address};

/// The menu's width floor in nominal pixels, so a host's meta keeps its trailing column as hosts join.
pub const MENU_MIN: f32 = 340.;
pub const ADD_SHARE: &str = "addShare";
const HOST: &str = "nearby:";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Looking,
    Found,
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub tailnet: bool,
    pub address: Address,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    pub id: usize,
    pub state: State,
    pub hosts: Vec<Host>,
}

/// The discovery request the pane's backend answers; stop ends a scan the menu no longer shows.
pub fn request(op: &str, id: usize) -> String {
    format!(r#"{{"c":"nearby","op":"{op}","id":{id}}}"#)
}

/// The backend's nearby line; "looking" acknowledges the start and carries no rows.
/// Sample input: {"t":"nearby","id":3,"state":"found","message":"","rows":[{"name":"NAS","source":"mdns","uri":"smb://nas.local/"}],"errors":[]}
pub fn parse(line: &str) -> Option<Reply> {
    if !line.contains("\"nearby\"") {
        return None;
    }
    let doc = jsondoc::parse(line).ok()?;
    if doc.get("t").and_then(Json::as_str) != Some("nearby") {
        return None;
    }
    let id = doc.get("id").and_then(Json::as_f64)? as usize;
    let state = match doc.get("state").and_then(Json::as_str)? {
        "looking" => State::Looking,
        "found" => State::Found,
        _ => State::Empty,
    };
    let mut hosts: Vec<Host> = doc
        .get("rows")
        .and_then(Json::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(|row| {
            let name = row.get("name").and_then(Json::as_str)?;
            let address = networkaddress::parse(row.get("uri").and_then(Json::as_str)?).ok()?;
            (!name.is_empty()).then(|| Host {
                name: name.to_owned(),
                tailnet: row.get("source").and_then(Json::as_str) == Some("tailscale"),
                address,
            })
        })
        .collect();
    // The backend sorts by name; the menu lists this network's hosts first and the tailnet's after, as the boards draw them.
    hosts.sort_by_key(|host| host.tailnet);
    let state = if state == State::Found && hosts.is_empty() {
        State::Empty
    } else {
        state
    };
    Some(Reply { id, state, hosts })
}

/// The card's protocol chip a scheme selects, named as the chip names it.
fn protocol_key(scheme: &str) -> &'static str {
    match scheme {
        "sftp" => "network.sftp",
        "ftp" | "ftps" => "network.ftps",
        "dav" | "davs" => "network.webdav",
        "nfs" => "network.nfs",
        _ => "network.smb",
    }
}

/// A host's muted trailing column: its protocol, then its address on this network or Tailscale for a tailnet peer.
pub fn meta(host: &Host, catalog: &I18n) -> String {
    let protocol = catalog.tr(protocol_key(&host.address.scheme));
    let place = if host.tailnet {
        catalog.tr("network.tailscale")
    } else {
        host.address.host.clone()
    };
    catalog.tr_args("network.host_caption", &[&protocol, &place])
}

/// The menu's rows: the hosts, or the busy row while it still looks, then Add a Network Share… after a break, always last.
pub fn entries(state: State, hosts: &[Host], catalog: &I18n) -> Vec<Entry> {
    let mut out: Vec<Entry> = match state {
        State::Looking => vec![Entry {
            busy: true,
            ..Entry::leaf("nearbyLooking", Text::Key("network.looking_hosts"))
        }],
        State::Found => hosts
            .iter()
            .enumerate()
            .map(|(index, host)| {
                let mut entry = Entry::row(
                    format!("{HOST}{index}"),
                    Text::Data(host.name.clone()),
                    "server",
                );
                if host.tailnet {
                    entry.brand = Some(Brand::Tailscale);
                }
                entry.hint = Some(Text::Data(meta(host, catalog)));
                entry
            })
            .collect(),
        State::Empty => Vec::new(),
    };
    if !out.is_empty() {
        out.push(Entry::separator());
    }
    out.push(Entry::row(
        ADD_SHARE,
        Text::Key("network.add_share"),
        "plus",
    ));
    out
}

/// The host a fired row names, by its index in the answer the menu drew.
pub fn chosen<'a>(action: &str, hosts: &'a [Host]) -> Option<&'a Host> {
    hosts.get(action.strip_prefix(HOST)?.parse::<usize>().ok()?)
}

#[cfg(test)]
#[path = "nearby_tests.rs"]
mod tests;
