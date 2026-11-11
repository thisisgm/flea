//! The saved-place edit arm survives refusal and never rewrites an unchanged address.
use super::dialog::{Dialog, Effect};

#[test]
fn uish_misc_saved_location_retry_with_changed_uri_closes_without_save() {
    let mut dialog = Dialog::default();
    dialog.open_location("smb://user@old.test/share", "Share", false);
    dialog.form.host = "corrected.test".into();
    dialog.password.push_str("synthetic");
    let effects = dialog.submit();
    assert!(
        matches!(effects.as_slice(), [Effect::Mount { uri, .. }] if uri == "smb://user@corrected.test/share")
    );
    let uri = dialog.pending_uri.clone();
    let effects = dialog.mount_finished(dialog.request, &uri, Ok(()), false);
    assert!(effects
        .iter()
        .any(|effect| matches!(effect, Effect::Closed)));
    assert!(!effects
        .iter()
        .any(|effect| matches!(effect, Effect::Save { .. })));
    assert!(!dialog.opened && !dialog.saving && !dialog.save_new);
    assert!(
        dialog.replace.is_none(),
        "Retry is separate from Edit Address"
    );
}

#[test]
fn uish_misc_native8_network_retry_retains_only_the_live_masked_draft() {
    for key in [
        "network.no_prompt",
        "network.no_answer",
        "network.refused",
        "network.password_refused",
    ] {
        let mut d = Dialog::default();
        d.open();
        d.form.host = "host".into();
        d.form.user = "user".into();
        d.password.push_str("synthetic-secret");
        d.reveal = true;
        let effects = d.submit();
        assert!(
            matches!(effects.as_slice(), [Effect::Mount { password, .. }] if password.expose() == b"synthetic-secret")
        );
        let uri = d.pending_uri.clone();
        d.mount_finished(d.request, &uri, Err(key), true);
        assert!(d.opened && d.retry && d.failed_connect && !d.reveal);
        assert_eq!(d.password.expose(), b"synthetic-secret", "{key}");
        assert!(
            matches!(d.submit().as_slice(), [Effect::Mount { password, .. }] if password.expose() == b"synthetic-secret")
        );
        d.close();
        assert!(d.password.is_empty(), "closing retires the draft");
        d.open();
        assert!(
            d.password.is_empty(),
            "reopening has no retained credential"
        );
    }
}

#[test]
fn pl2_edit_address_is_armed_until_its_own_successful_changed_mount() {
    let mut d = Dialog::default();
    d.open_location("smb://nas:445/share/", "Saved", true);
    assert_eq!(
        d.status, None,
        "Edit Address opens with no status line, as v0.3.12 draws it"
    );
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
    use crate::g2::device_rail::secret::Secret;
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
