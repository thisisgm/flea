//! The saved-place edit arm survives refusal and never rewrites an unchanged address.
use super::dialog::{Dialog, Effect};

#[test]
fn pl2_edit_address_is_armed_until_its_own_successful_changed_mount() {
    let mut d = Dialog::default();
    d.open_location("smb://nas:445/share/", "Saved", true);
    assert_eq!(d.status, Some("network.edit_prompt"));
    d.submit();
    let id = d.request;
    let uri = d.pending_uri.clone();
    assert!(d
        .mount_finished(id, &uri, Ok(()), false)
        .iter()
        .any(|e| matches!(e, Effect::Closed)));
    assert!(!d.opened, "unchanged normalized address saves nothing");
    d.open_location("smb://nas/share", "Saved", true);
    d.form.host = "corrected".into();
    d.submit();
    let uri = d.pending_uri.clone();
    d.mount_finished(d.request, &uri, Err("network.refused"), true);
    assert_eq!(d.replace.as_deref(), Some("smb://nas/share"));
    assert!(d.opened && !d.saving);
    d.submit();
    let effects = d.mount_finished(d.request, &uri, Ok(()), false);
    assert!(
        matches!(effects.as_slice(), [Effect::Save { replace: Some(old), uri, .. }] if old == "smb://nas/share" && uri == "smb://corrected/share")
    );
}

#[test]
fn pl2_editing_endpoint_after_save_refusal_requires_a_new_mount() {
    let mut d = Dialog::default();
    d.open();
    d.form.host = "first".into();
    d.form.path = "share".into();
    d.submit();
    let uri = d.pending_uri.clone();
    d.mount_finished(d.request, &uri, Ok(()), false);
    d.saved(d.request, false, "Writer busy");
    d.form.host = "second".into();
    assert!(
        matches!(d.submit().as_slice(), [Effect::Mount { uri, .. }] if uri == "smb://second/share")
    );
}

#[test]
fn pl2_secret_positions_handle_non_bmp_selection_and_replacement() {
    use super::secret_edit::*;
    use crate::g2::pl1::secret::Secret;
    let mut secret = Secret::from_bytes("a\u{1d11e}猫z".as_bytes());
    assert_eq!(units(&secret), 5);
    assert_eq!(previous(&secret, 3), 1);
    assert_eq!(next(&secret, 1), 3);
    assert_eq!(display_index(&secret, 3, true), 5);
    assert_eq!(display_index(&secret, 3, false), 6);
    assert_eq!(replace(&mut secret, 1..4, "x"), 2..2);
    assert_eq!(secret.expose(), b"axz");
}

#[test]
fn pl2_reopening_a_failed_location_acknowledges_its_real_cause_once() {
    let mut d = Dialog::default();
    d.open_location("smb://user@host/share", "Saved", false);
    d.password.push_str("fixture-password");
    d.submit();
    let uri = d.pending_uri.clone();
    d.mount_finished(d.request, &uri, Err("network.no_prompt"), true);
    d.close();
    d.open_location("smb://user@host:445/share/", "Saved", false);
    assert_eq!(d.status, Some("network.no_prompt"));
    assert!(d.failed_connect);
    d.close();
    d.open_location(&uri, "Saved", false);
    assert_eq!(d.status, None, "showing the retained cause acknowledges it");
    d.close();
    d.open();
    assert_eq!(d.status, None);
}

#[test]
fn pl2_backend_death_cancels_connect_but_never_replays_an_accepted_save() {
    let mut d = Dialog::default();
    d.open();
    d.form.host = "host".into();
    d.submit();
    let id = d.request;
    let effects = d.backend_died();
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::Cancel(n) if *n == id)));
    assert!(effects
        .iter()
        .any(|e| matches!(e, Effect::CancelDropbox(_))));
    d.close();
    d.open();
    d.form.host = "host".into();
    d.submit();
    let uri = d.pending_uri.clone();
    d.mount_finished(d.request, &uri, Ok(()), false);
    let id = d.request;
    d.backend_died();
    assert!(d.saving);
    assert!(d.submit().is_empty());
    assert!(d.close().is_empty());
    assert!(d
        .saved(id, true, "")
        .iter()
        .any(|e| matches!(e, Effect::Closed)));
}
