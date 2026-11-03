// Issue 133, the TUI's half of ui/js/Mounts.js "trashable": what a folder's path says about its Trash.
pub use crate::backend::hometrash::trashable;

// The status line's refusal: what failed, then the key that still removes the rows, when the keymap binds one.
#[cfg(test)]
#[allow(dead_code)]
pub fn no_trash(delete_permanently: &str) -> String { no_trash_text(delete_permanently).to_string() }
pub fn no_trash_text(delete_permanently: &str) -> super::words::Text {
    if delete_permanently.is_empty() { super::words::Text::Key("feedback.no_trash") }
    else { super::words::args("tui.mounts.no_trash_key", vec![delete_permanently.into()]) }
}
