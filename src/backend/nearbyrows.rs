use super::networkaddress::{self, Address};
use crate::json::escape;
use std::collections::BTreeMap;

#[path = "nearbystatus.rs"]
mod status;

pub const MAX_ROWS: usize = 256;
const MAX_LINE: usize = 8192;
const AVAHI_FIELDS: usize = 10;
pub const SERVICES: [&str; 6] = ["_smb._tcp", "_sftp-ssh._tcp", "_webdav._tcp", "_webdavs._tcp", "_nfs._tcp", "_nfs._udp"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub source: &'static str,
    pub address: Address,
}

impl Row {
    pub fn json(&self) -> String {
        format!(r#"{{"name":"{}","source":"{}",{}}}"#, escape(&self.name), self.source, networkaddress::fields(&self.address))
    }
}

fn service_scheme(service: &str) -> Option<&'static str> {
    Some(match service {
        "_smb._tcp" => "smb",
        "_sftp-ssh._tcp" => "sftp",
        "_webdav._tcp" => "dav",
        "_webdavs._tcp" => "davs",
        "_nfs._tcp" | "_nfs._udp" => "nfs",
        _ => return None,
    })
}

// Avahi emits backslash plus three decimal digits for escaped label and TXT bytes.
// Sample input: NAS\032office\059one
fn unescape(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'\\' { out.push(bytes[at]); at += 1; continue; }
        at += 1;
        if bytes.get(at).is_some_and(u8::is_ascii_digit) {
            let digits = bytes.get(at..at + 3)?;
            if !digits.iter().all(u8::is_ascii_digit) { return None; }
            let value = digits.iter().fold(0u16, |n, b| n * 10 + (b - b'0') as u16);
            out.push(u8::try_from(value).ok()?);
            at += 3;
        } else {
            out.push(*bytes.get(at)?);
            at += 1;
        }
    }
    let out = String::from_utf8(out).ok()?;
    (!out.chars().any(char::is_control)).then_some(out)
}

// Sample input: "note=ignored" "path=/docs\032and\032photos"
fn txt_path(input: &str) -> Option<String> {
    let mut rest = input.trim();
    while let Some(quoted) = rest.strip_prefix('"') {
        let mut escaped = false;
        let end = quoted.bytes().position(|b| {
            if escaped { escaped = false; false }
            else if b == b'\\' { escaped = true; false }
            else { b == b'"' }
        })?;
        if let Some(raw_path) = quoted[..end].strip_prefix("path=") {
            let path = unescape(raw_path)?;
            return Some(if path.starts_with('/') { path } else { format!("/{path}") });
        }
        rest = quoted[end + 1..].trim_start();
    }
    Some("/".into())
}

// Discovery rejects underscores only to preserve its existing behavior; typed addresses keep legacy underscore support.
// Sample input: nas-1.local
fn valid_discovery_host(host: &str) -> bool {
    !host.contains('_') && networkaddress::valid_host(host)
}

// Sample input: =;eth0;IPv4;NAS;_smb._tcp;local;nas.local;192.0.2.1;445;"path=/docs"
pub fn avahi(text: &str, service: &str) -> Vec<Row> {
    let Some(scheme) = service_scheme(service) else { return Vec::new(); };
    let mut rows = Vec::new();
    for line in text.lines().filter(|line| line.len() <= MAX_LINE) {
        let fields: Vec<&str> = line.splitn(AVAHI_FIELDS, ';').collect();
        if fields.len() != AVAHI_FIELDS || fields[0] != "=" || fields[4] != service { continue; }
        let host = fields[6].strip_suffix('.').unwrap_or(fields[6]);
        if !valid_discovery_host(host) { continue; }
        let Some(port) = networkaddress::decimal_port(fields[8]) else { continue; };
        let Some(name) = unescape(fields[3]).filter(|name| !name.is_empty()) else { continue; };
        let Some(path) = txt_path(fields[9]) else { continue; };
        let uri = format!("{scheme}://{host}:{port}{}", networkaddress::encode(&path, true));
        let Ok(address) = networkaddress::parse(&uri) else { continue; };
        rows.push(Row { name, source: "mdns", address });
        if rows.len() == MAX_ROWS { break; }
    }
    rows
}

// Sample input: {"BackendState":"Running","Peer":{}}
#[cfg(test)]
pub fn tailscale(input: &str) -> Result<Vec<Row>, &'static str> {
    tailscale_checked(input, &mut || true)
}

// Sample input: {"BackendState":"Running","Peer":{}}
pub fn tailscale_checked(input: &str, keep_going: &mut dyn FnMut() -> bool) -> Result<Vec<Row>, &'static str> {
    status::decode(input, keep_going)
}

// An endpoint appears once across interfaces and address families; distinct paths and protocols stay distinct.
pub fn deduplicate(rows: Vec<Row>) -> Vec<Row> {
    let mut found = BTreeMap::new();
    for row in rows {
        let key = row.address.uri.clone();
        if found.contains_key(&key) || found.len() < MAX_ROWS {
            found.entry(key).and_modify(|old: &mut Row| {
                if (row.source, &row.name) < (old.source, &old.name) { *old = row.clone(); }
            }).or_insert(row);
        }
    }
    let mut rows: Vec<Row> = found.into_values().collect();
    rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.address.uri.cmp(&b.address.uri)));
    rows
}

#[cfg(test)]
#[path = "nearbyrows_tests.rs"]
mod tests;
