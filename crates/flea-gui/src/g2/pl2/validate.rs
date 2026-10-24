//! Saved supported addresses cross the same parser as typed ones before they can reach argv.
use super::mount::{Effect, Request};
use flea::{ui_logic_pl1::Note, ui_logic_pl2::form::Form};

pub(super) fn refused(request: &Request) -> Option<Vec<Effect>> {
    if Form::saved_address(&request.uri).is_ok() {
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
    entry: &flea::ui_logic_pl1::RailEntry,
    listing: &str,
) -> Option<String> {
    let uri = &entry.uri;
    if let Some(raw) = uri.strip_prefix("file://") {
        // A kernel row gio lends is released by its listed URI only when that URI names the row's own path.
        let decoded = flea::ui_logic_pl1::mounts::decode_path(raw);
        return (valid_mount_path(&entry.path) && decoded.trim_end_matches('/') == entry.path)
            .then(|| uri.clone());
    }
    if uri.starts_with('/') {
        return valid_mount_path(uri)
            .then(|| uri.clone())
            .filter(|_| *uri == entry.path);
    }
    let saved = Form::saved_address(uri).ok()?.uri;
    let release = flea::ui_logic_pl2::rail::release_uri(listing, &saved);
    let release = Form::saved_address(&release).ok()?.uri;
    (flea::ui_logic_pl2::mounts::normalize(&saved)
        == flea::ui_logic_pl2::mounts::normalize(&release)
        || flea::ui_logic_pl2::rail::covers(&release, &saved))
    .then_some(release)
}

pub(super) fn valid_mount_path(path: &str) -> bool {
    path.starts_with('/')
        && path != "/"
        && path.len() <= 8192
        && !path.chars().any(char::is_control)
}
