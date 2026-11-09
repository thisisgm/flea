use super::*;
use crate::ui_logic_file_clipboard::harness::english;

#[test]
fn paste_image_selection_changes_preserve_file_and_image_distinction() {
    let mut session = Session::new();
    session.foreign("image", Vec::new(), String::new());
    assert!(session.image_available());
    assert!(!session.available());
    session.foreign("none", Vec::new(), String::new());
    assert!(!session.image_available());
    session.foreign("image", Vec::new(), String::new());
    session.set(0, vec!["/files/a".into()], false);
    assert!(!session.image_available());
    assert!(session.available());
}

#[test]
fn paste_image_late_read_cannot_enable_paste_for_an_old_offer() {
    let mut session = Session::new();
    let en = english();
    session.on_changed(0, "none", Vec::new(), String::new(), "watch lost");
    session.read_for_paste(&en, 0, PendingPaste::transfer("/files"));
    session.foreign("none", Vec::new(), String::new());
    session.on_get(0, true, "image", Vec::new(), String::new());
    assert!(!session.image_available());
    session.read_for_paste(&en, 0, PendingPaste::transfer("/files"));
    session.on_get(0, true, "image", Vec::new(), String::new());
    assert!(session.image_available());
}
