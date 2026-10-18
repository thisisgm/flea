use super::{networkaddress, Row, MAX_ROWS};
use std::collections::BTreeMap;

#[path = "nearbyjson.rs"]
mod stream;

// Sample input: {"BackendState":"Running","Peer":{"one":{"Online":true,"DNSName":"nas.tail.ts.net."}}}
pub(super) fn decode(input: &str, keep_going: &mut dyn FnMut() -> bool) -> Result<Vec<Row>, &'static str> {
    let mut state = "";
    let mut peers = "{}";
    stream::object(input, keep_going, |key, raw, _| {
        match key.as_str() {
            "BackendState" => state = raw,
            "Peer" => peers = raw,
            _ => {}
        }
        Ok(())
    })?;
    if stream::text(state).as_deref() != Some("Running") || !peers.starts_with('{') { return Ok(Vec::new()); }
    let mut rows = BTreeMap::new();
    stream::object(peers, keep_going, |id, raw, keep_going| {
        match peer(raw, keep_going)? {
            Some(row) if rows.contains_key(&id) || rows.len() < MAX_ROWS => { rows.insert(id, row); }
            None => { rows.remove(&id); }
            _ => {}
        }
        Ok(())
    })?;
    Ok(rows.into_values().collect())
}

// Sample input: {"Online":true,"DNSName":"nas.tail.ts.net.","HostName":"NAS","OS":"windows"}
fn peer(input: &str, keep_going: &mut dyn FnMut() -> bool) -> Result<Option<Row>, &'static str> {
    if !input.starts_with('{') { return Ok(None); }
    let (mut online, mut dns, mut label, mut os) = (false, None, None, None);
    stream::object(input, keep_going, |key, raw, _| {
        match key.as_str() {
            "Online" => online = raw == "true",
            "DNSName" => dns = stream::text(raw),
            "HostName" => label = stream::text(raw),
            "OS" => os = stream::text(raw),
            _ => {}
        }
        Ok(())
    })?;
    let Some(dns) = dns.filter(|_| online) else { return Ok(None); };
    let dns = dns.strip_suffix('.').unwrap_or(&dns);
    if !super::valid_discovery_host(dns) || dns.to_ascii_lowercase().ends_with(".mullvad.ts.net") { return Ok(None); }
    let scheme = if os.is_some_and(|os| os.eq_ignore_ascii_case("windows")) { "smb" } else { "sftp" };
    let Ok(address) = networkaddress::parse(&format!("{scheme}://{dns}/")) else { return Ok(None); };
    let name = label.filter(|label| !label.is_empty() && !label.eq_ignore_ascii_case("localhost"))
        .unwrap_or_else(|| dns.split('.').next().unwrap_or(dns).into());
    if name.chars().any(char::is_control) { return Ok(None); }
    Ok(Some(Row { name, source: "tailscale", address }))
}

#[cfg(test)]
#[path = "nearbystatus_tests.rs"]
mod tests;
