//! Saved supported addresses cross the same parser as typed ones before they can reach argv.
use super::mount::{Effect, Request};
use flea::{ui_logic_device_rail::Note, ui_logic_network_shares::form::Form};

/// The longest phone URI or mount path that may reach argv.
const MAX_URI_BYTES: usize = 8192;

/// A phone root "gio mount -li" offered: gvfs's MTP, AFC or GPhoto2 transport, opened like a share (ui/Sidebar.qml:385).
pub(super) fn phone(uri: &str) -> bool {
    let Some((scheme, rest)) = uri.split_once("://") else {
        return false;
    };
    matches!(scheme, "mtp" | "afc" | "gphoto2")
        && !rest.trim_end_matches('/').is_empty()
        && uri.len() <= MAX_URI_BYTES
        && !uri.chars().any(|ch| ch.is_control() || ch.is_whitespace())
}

pub(super) fn refused(request: &Request) -> Option<Vec<Effect>> {
    if phone(&request.uri)
        || Form::saved_address(&request.uri).is_ok()
        || (request.id == 0 && super::routing::incomplete(&request.uri).is_some())
    {
        return None;
    }
    Some(if request.id == 0 {
        vec![Effect::Say {
            note: Note::new("network.invalid_address", &[]),
            origin: request.origin,
        }]
    } else {
        vec![Effect::Completed {
            id: request.id,
            uri: String::new(),
            error: Some("network.invalid_address"),
            args: Vec::new(),
            connect_failed: false,
        }]
    })
}

pub(super) fn release_identity(
    entry: &flea::ui_logic_device_rail::RailEntry,
    listing: &str,
) -> Option<String> {
    let uri = &entry.uri;
    if let Some(raw) = uri.strip_prefix("file://") {
        // A kernel row gio lends is released by its listed URI only when that URI names the row's own path.
        let decoded = flea::ui_logic_device_rail::mounts::decode_path(raw);
        return (valid_mount_path(&entry.path) && decoded.trim_end_matches('/') == entry.path)
            .then(|| uri.clone());
    }
    if uri.starts_with('/') {
        return valid_mount_path(uri)
            .then(|| uri.clone())
            .filter(|_| *uri == entry.path);
    }
    let saved = Form::saved_address(uri).ok()?.uri;
    let release = flea::ui_logic_network_shares::rail::release_uri(listing, &saved);
    let release = Form::saved_address(&release).ok()?.uri;
    (flea::ui_logic_network_shares::mounts::normalize(&saved)
        == flea::ui_logic_network_shares::mounts::normalize(&release)
        || flea::ui_logic_network_shares::rail::covers(&release, &saved))
    .then_some(release)
}

pub(super) fn valid_mount_path(path: &str) -> bool {
    path.starts_with('/')
        && path != "/"
        && path.len() <= MAX_URI_BYTES
        && !path.chars().any(char::is_control)
}
