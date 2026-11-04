//! The uri pieces the rail keys and labels on: scheme, authority, host and the default port gio drops.

/// The ports gio leaves out of a listed uri, and nfs's own default.
pub const SCHEME_PORTS: &[(&str, u16)] =
    &[("smb", 445), ("sftp", 22), ("ftp", 21), ("ftps", 21), ("dav", 80), ("davs", 443), ("nfs", 2049)];

/// The scheme lowercased, "" when the text is not a uri.
pub fn scheme_of(uri: &str) -> String {
    uri.find("://").map(|mark| uri[..mark].to_lowercase()).unwrap_or_default()
}

/// Everything between :// and the next slash.
pub fn authority(uri: &str) -> &str {
    let Some(mark) = uri.find("://") else { return "" };
    let rest = &uri[mark + 3..];
    rest.find('/').map_or(rest, |slash| &rest[..slash])
}

/// The host alone: userinfo and port dropped, a bracketed IPv6 literal kept whole.
pub fn host_of(uri: &str) -> &str {
    let auth = authority(uri);
    let host_port = auth.rfind('@').map_or(auth, |at| &auth[at + 1..]);
    if host_port.starts_with('[') {
        return host_port.find(']').map_or(host_port, |close| &host_port[..=close]);
    }
    host_port.find(':').map_or(host_port, |colon| &host_port[..colon])
}

fn default_port(scheme: &str) -> u16 {
    SCHEME_PORTS.iter().find(|(s, _)| *s == scheme).map_or(0, |(_, p)| *p)
}

/// One spelling for a share: the scheme's default port dropped from the authority.
pub fn strip_default_port(uri: &str) -> String {
    let auth = authority(uri);
    let port = default_port(&scheme_of(uri));
    let tail = format!(":{port}");
    if auth.is_empty() || port == 0 || !auth.ends_with(&tail) {
        return uri.to_string();
    }
    let mark = uri.find("://").map_or(0, |m| m + 3);
    format!("{}{}{}", &uri[..mark], &auth[..auth.len() - tail.len()], &uri[mark + auth.len()..])
}

/// gvfsd's "<share> <word> <host>" label cut back to the share when it ends in this uri's own host.
pub fn share_name(raw_label: &str, uri: &str) -> String {
    let host = host_of(uri);
    if host.is_empty() || raw_label.len() <= host.len() || !raw_label.ends_with(host) {
        return raw_label.to_string();
    }
    let head = &raw_label[..raw_label.len() - host.len()];
    let name = strip_connector(head);
    if !name.is_empty() && name != head { name.to_string() } else { raw_label.to_string() }
}

fn strip_connector(head: &str) -> &str {
    let body = head.trim_end();
    if body.len() == head.len() {
        return head;
    }
    let word = body.rfind(char::is_whitespace).map_or(0, |at| at + body[at..].chars().next().map_or(1, char::len_utf8));
    if word == 0 || word == body.len() {
        return head;
    }
    body[..word].trim_end()
}
