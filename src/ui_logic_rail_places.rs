//! Places, favorites, tags, recents and plugin rail data shared by the GPUI rail and the TUI.
pub mod favourites;
pub mod order;
pub mod places;
pub mod placeview;
pub mod plugin;
pub mod rail;
pub mod railkeys;
pub mod recent;
pub mod recentmode;
pub mod shelfpile;
pub mod tags;
pub mod tagtheme;
pub mod today;
pub mod userdirs;

#[cfg(test)]
mod favourites_tests;
#[cfg(test)]
mod fix1_tests;
#[cfg(test)]
mod fix2_tests;
#[cfg(test)]
mod heldby_tests;
#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod places_tests;
#[cfg(test)]
mod placeview_tests;
#[cfg(test)]
mod plugin_heading_tests;
#[cfg(test)]
mod plugin_tests;
#[cfg(test)]
mod railkeys_tests;
#[cfg(test)]
mod recent_tests;
#[cfg(test)]
mod recentmode_tests;
#[cfg(test)]
mod shelfpile_tests;
#[cfg(test)]
mod tags_tests;
#[cfg(test)]
mod tagtheme_tests;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod userdirs_tests;

// Sample input: "/home/gm/Downloads" names "Downloads"; a path with no slash is its own leaf.
pub fn leaf(path: &str) -> &str {
    path.rfind('/').map_or(path, |cut| &path[cut + 1..])
}

// Sample input: "%20" decodes to a space; a malformed escape or invalid UTF-8 is None, as decodeURIComponent throws.
pub fn decode_component(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = raw.get(at + 1..at + 3)?;
            if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(out).ok()
}

// The raw text when it cannot decode, the way Mounts.decodePath keeps a bookmark line readable.
pub fn decode_path(raw: &str) -> String {
    decode_component(raw).unwrap_or_else(|| raw.to_owned())
}

fn scheme_of(uri: &str) -> &str {
    uri.find("://").map_or("", |cut| &uri[..cut])
}

fn authority_of(uri: &str) -> &str {
    let Some(mark) = uri.find("://") else {
        return "";
    };
    let rest = &uri[mark + 3..];
    &rest[..rest.find('/').unwrap_or(rest.len())]
}

// The port gio drops per scheme, as ui/js/Protocols.js SCHEME_PORTS lists them; 0 means none.
fn default_port(scheme: &str) -> u16 {
    match scheme.to_ascii_lowercase().as_str() {
        "smb" => 445,
        "sftp" => 22,
        "ftp" | "ftps" => 21,
        "dav" => 80,
        "davs" => 443,
        "nfs" => 2049,
        _ => 0,
    }
}

// Sample input: "smb://nas:445/isos" drops the port its scheme implies.
pub fn strip_default_port(uri: &str) -> String {
    let auth = authority_of(uri);
    let port = default_port(scheme_of(uri));
    let tail = format!(":{port}");
    if auth.is_empty() || port == 0 || !auth.ends_with(&tail) {
        return uri.to_owned();
    }
    let mark = uri.find("://").map_or(0, |m| m + 3);
    format!(
        "{}{}{}",
        &uri[..mark],
        &auth[..auth.len() - tail.len()],
        &uri[mark + auth.len()..]
    )
}

// Sample input: "smb://host/data/" and "smb://host:445/data" both normalize to "smb://host/data"; a bare root keeps its slash.
pub fn normalize(uri: &str) -> String {
    let stripped = strip_default_port(uri.trim_end_matches('/'));
    let scheme = scheme_of(&stripped);
    let bare_root = !scheme.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        && !authority_of(&stripped).is_empty()
        && stripped.len() == scheme.len() + 3 + authority_of(&stripped).len();
    if bare_root {
        stripped + "/"
    } else {
        stripped
    }
}

// Sample input: ("/home/gm/Documents/x", "/home/gm/Documents") is inside; a name prefix sibling is not.
pub fn inside(dir: &str, root: &str) -> bool {
    dir == root
        || dir
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}

// Sample input: "/run/user/1000/gvfs/smb-share:server=nas,share=data" sits under gvfs.
pub fn under_gvfs(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/run/user/") else {
        return false;
    };
    let Some((user, rest)) = rest.split_once('/') else {
        return false;
    };
    let Some(rest) = rest.strip_prefix("gvfs/") else {
        return false;
    };
    let Some((scheme, _)) = rest.split_once(':') else {
        return false;
    };
    !user.is_empty()
        && !scheme.is_empty()
        && scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

// A control character, which no path drawn or sent back as a URI may hold.
pub fn has_control(text: &str) -> bool {
    text.chars().any(|c| (c as u32) < 0x20 || c == '\u{7f}')
}

// JavaScript's WhiteSpace and LineTerminator set, which String.prototype.trim strips: U+FEFF is one, U+0085 is not.
pub fn js_space(c: char) -> bool {
    c == '\u{feff}' || (c != '\u{85}' && c.is_whitespace())
}

// Sample input: "\u{feff} Docs " trims to "Docs" and "\u{85}" stays, as String.prototype.trim reads them.
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(js_space)
}
