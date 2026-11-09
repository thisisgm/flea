//! Which rail row takes a drop and into which folder (RailDrop.js destination).
use super::{refusal, sources};

/// What kind of rail row a drop rests on; only a folder row can be a destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailKind {
    Folder,
    Other,
}

// Sample input: "/run/media/gm/USB/" is "/run/media/gm/USB" (DropDev.normalize).
fn normalize(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() && path.starts_with('/') {
        "/"
    } else {
        trimmed
    }
}

// A phone root under gvfs takes no drop.
fn phone_root(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("/run/user/") else {
        return false;
    };
    let Some((uid, rest)) = rest.split_once('/') else {
        return false;
    };
    let Some(name) = rest.strip_prefix("gvfs/") else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    !uid.is_empty()
        && uid.bytes().all(|b| b.is_ascii_digit())
        && ["mtp:", "gphoto2:", "afc:"]
            .iter()
            .any(|scheme| lower.starts_with(scheme) && lower.len() > scheme.len())
        && !name.contains('/')
}

/// The folder a rail row takes a drop into, or "" when it takes none (RailDrop.destination).
pub fn rail_destination(
    kind: RailKind,
    path: &str,
    mounted: bool,
    urls: &[String],
    plain: Option<&str>,
    has_marker: bool,
) -> String {
    if kind != RailKind::Folder || !mounted {
        return String::new();
    }
    let path = normalize(path);
    if !path.starts_with('/') || phone_root(path) {
        return String::new();
    }
    let paths = sources(urls, plain, has_marker);
    if paths.is_empty() || refusal(&paths, path, has_marker).is_some() {
        return String::new();
    }
    path.to_owned()
}
