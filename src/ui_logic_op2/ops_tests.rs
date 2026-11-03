//! Port of the submit flows: secrets to the wire, stickies only on a live send, empty selections said.
use super::{english, ops::*};

#[test]
fn empty_selections_say_so_and_open_no_card() {
    assert_eq!(open_compress_password(&[]), Ask::NothingToActOn);
    assert_eq!(open_compress_password(&[0, 1]), Ask::Paths);
    assert_eq!(compress_password_resolved(&[]), Ask::NothingToActOn);
    assert_eq!(compress_password_resolved(&["/d/a"]), Ask::Paths);
}

#[test]
fn submit_carries_the_secret_to_the_wire_never_the_card() {
    let i18n = english();
    let submit = submit_compress_password(&i18n, &CompressRequest { paths: &["/d/a", "/d/b"], dest: "/d/Archive.7z", format: "7z", password: "s3cret", encrypt_names: true, menu_id: 3 }, false).unwrap();
    assert!(submit.line.contains("\"password\":\"s3cret\""));
    assert!(submit.line.contains("\"menuId\":3"));
    assert_eq!(submit.sticky.as_deref(), Some("Compressing 2 items to .7z"));
}

#[test]
fn queueing_send_leaves_no_stale_sticky_line() {
    let i18n = english();
    let submit = submit_compress_password(&i18n, &CompressRequest { paths: &["/d/a"], dest: "/d/Archive.7z", format: "7z", password: "s3cret", encrypt_names: true, menu_id: 0 }, true).unwrap();
    assert!(submit.sticky.is_none());
}

#[test]
fn empty_submit_sends_nothing() {
    let i18n = english();
    assert!(submit_compress_password(&i18n, &CompressRequest { paths: &[], dest: "/d/a.7z", format: "7z", password: "s3cret", encrypt_names: true, menu_id: 0 }, false).is_none());
}

#[test]
fn no_target_says_so() {
    let i18n = english();
    assert_eq!(nothing_to_act_on(&i18n), "There is nothing to act on.");
}

#[test]
fn retry_goes_to_the_source_the_card_opened_over() {
    let line = submit_extract_password(Some("/d/backup.7z"), "/d/backup", "s3cret", 0).unwrap();
    assert!(line.contains("\"path\":\"/d/backup.7z\"") && line.contains("\"password\":\"s3cret\""));
    assert!(submit_extract_password(None, "/d/backup", "s3cret", 0).is_none());
    assert!(submit_extract_password(Some(""), "/d/backup", "s3cret", 0).is_none());
}
