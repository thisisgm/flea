use crate::json::escape;
use std::net::Ipv6Addr;

pub const INVALID: &str = "Enter a valid network address.";
const MAX_ADDRESS: usize = 8192;
const MAX_HOST: usize = 253;
const MAX_LABEL: usize = 63;

// Both front ends hand uri to their saved-place opener; parsing never mounts or saves a host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub uri: String,
    pub scheme: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub domain: String,
    pub path: String,
}

pub fn protocol(scheme: &str) -> Option<(&'static str, u16)> {
    Some(match scheme {
        "smb" => ("SMB", 445),
        "sftp" => ("SFTP", 22),
        "ftps" => ("FTPS", 21),
        "dav" => ("WebDAV", 80),
        "davs" => ("WebDAV", 443),
        "nfs" => ("NFS", 2049),
        _ => return None,
    })
}

// Sample input: nas-1.local
pub fn valid_host(host: &str) -> bool {
    if let Some(ip) = host.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return ip.parse::<Ipv6Addr>().is_ok();
    }
    !host.is_empty() && host.len() <= MAX_HOST && host.split('.').all(|label| {
        !label.is_empty() && label.len() <= MAX_LABEL
            && !label.starts_with('-') && !label.ends_with('-')
            && label.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    })
}

// Sample input: 00445
pub fn decimal_port(input: &str) -> Option<u16> {
    if input.is_empty() || !input.bytes().all(|b| b.is_ascii_digit()) { return None; }
    input.parse::<u16>().ok().filter(|&port| port != 0)
}

// Sample input: sftp://gm@nas.local:2222/docs%20and%20photos
pub fn parse(input: &str) -> Result<Address, &'static str> {
    if input.len() > MAX_ADDRESS || input.chars().any(char::is_control) { return Err(INVALID); }
    let input = input.trim();
    if input.is_empty() {
        return Err(INVALID);
    }
    let (scheme, authority, raw_path, encoded) = if let Some((scheme, rest)) = input.split_once("://") {
        let slash = rest.find('/').unwrap_or(rest.len());
        (scheme.to_ascii_lowercase(), &rest[..slash], &rest[slash..], true)
    } else {
        let at = input.find('@').ok_or(INVALID)?;
        let split = if input[at + 1..].starts_with('[') {
            at + 1 + input[at + 1..].find(']').ok_or(INVALID)? + 1
        } else {
            at + 1 + input[at + 1..].find(':').ok_or(INVALID)?
        };
        let path = input.get(split..).and_then(|s| s.strip_prefix(':')).ok_or(INVALID)?;
        if !path.starts_with('/') { return Err(INVALID); }
        ("sftp".into(), &input[..split], path, false)
    };
    let (label, default_port) = protocol(&scheme).ok_or(INVALID)?;
    let (userinfo, host_port) = match authority.split_once('@') {
        Some((user, host)) if !user.is_empty() && !host.contains('@') => (user, host),
        Some(_) => return Err(INVALID),
        None => ("", authority),
    };
    let (host, port) = if host_port.starts_with('[') {
        let end = host_port.find(']').ok_or(INVALID)? + 1;
        (&host_port[..end], &host_port[end..])
    } else {
        match host_port.find(':') {
            Some(at) => (&host_port[..at], &host_port[at..]),
            None => (host_port, ""),
        }
    };
    let host = host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase();
    if !valid_host(&host) { return Err(INVALID); }
    let port = if port.is_empty() { default_port } else {
        let digits = port.strip_prefix(':').ok_or(INVALID)?;
        decimal_port(digits).ok_or(INVALID)?
    };
    if encoded && (raw_path.contains('?') || raw_path.contains('#')) { return Err(INVALID); }
    let raw_path = raw_path.trim_end_matches('/');
    let raw_path = if raw_path.is_empty() { "/" } else { raw_path };
    let path = if encoded { decode(raw_path)? } else { raw_path.to_string() };
    let (domain, user) = if scheme == "smb" {
        match userinfo.split_once(';') {
            Some((domain, user)) if !domain.is_empty() && !user.is_empty() => (domain, user),
            Some(_) => return Err(INVALID),
            None => ("", userinfo),
        }
    } else { ("", userinfo) };
    if user.contains(':') || (scheme == "nfs" && !user.is_empty()) { return Err(INVALID); }
    let user = if encoded { decode(user)? } else { user.into() };
    let domain = decode(domain)?;
    if user.contains(':') { return Err(INVALID); }
    let info = if user.is_empty() { String::new() } else {
        let domain = if domain.is_empty() { String::new() } else { format!("{};", encode(&domain, false)) };
        format!("{}{}@", domain, encode(&user, false))
    };
    let port_text = if port == default_port { String::new() } else { format!(":{port}") };
    let uri_path = if encoded { encoded_path(raw_path) } else { encode(&path, true) };
    let uri = format!("{scheme}://{info}{host}{port_text}{uri_path}");
    Ok(Address { uri, scheme, protocol: label.into(), host, port, user, domain, path })
}

pub fn encode(input: &str, path: bool) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') || (path && b == b'/') {
            out.push(b as char);
        } else {
            use std::fmt::Write;
            write!(out, "%{b:02X}").ok();
        }
    }
    out
}

// Preserve existing escapes so an encoded separator or dot never becomes a different resource.
// Sample input: /docs%2fphotos
fn encoded_path(input: &str) -> String {
    let mut out = String::new();
    let mut bytes = input.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%' {
            out.push('%');
            out.push(bytes.next().expect("validated escape").to_ascii_uppercase() as char);
            out.push(bytes.next().expect("validated escape").to_ascii_uppercase() as char);
        } else if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(b as char);
        } else {
            use std::fmt::Write;
            write!(out, "%{b:02X}").ok();
        }
    }
    out
}

// Sample input: /docs%20and%20photos
fn decode(input: &str) -> Result<String, &'static str> {
    let mut out = Vec::new();
    let mut bytes = input.bytes();
    while let Some(b) = bytes.next() {
        out.push(if b == b'%' {
            let hi = (bytes.next().ok_or(INVALID)? as char).to_digit(16).ok_or(INVALID)?;
            let lo = (bytes.next().ok_or(INVALID)? as char).to_digit(16).ok_or(INVALID)?;
            (hi * 16 + lo) as u8
        } else { b });
    }
    let out = String::from_utf8(out).map_err(|_| INVALID)?;
    if out.chars().any(char::is_control) { return Err(INVALID); }
    Ok(out)
}

pub fn fields(address: &Address) -> String {
    format!(r#""uri":"{}","scheme":"{}","protocol":"{}","host":"{}","port":{},"user":"{}","domain":"{}","path":"{}","tls":{}"#,
        escape(&address.uri), address.scheme, address.protocol, escape(&address.host), address.port,
        escape(&address.user), escape(&address.domain), escape(&address.path), matches!(address.scheme.as_str(), "ftps" | "davs"))
}

pub fn answer(id: usize, input: &str) -> String {
    match parse(input) {
        Ok(address) => format!(r#"{{"t":"networkaddress","id":{id},"ok":true,{},"error":""}}"#, fields(&address)),
        Err(error) => format!(r#"{{"t":"networkaddress","id":{id},"ok":false,"uri":"","error":"{error}"}}"#),
    }
}

#[cfg(test)]
#[path = "networkaddress_tests.rs"]
mod tests;
