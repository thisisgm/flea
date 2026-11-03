//! Generic activation keys do not become row-specific menu hints.
use super::entry::{Entry, Text};

#[test]
fn generic_enter_stays_hidden_but_an_explicit_leaf_hint_survives() {
    let row = Entry::row("open", Text::Key("menu.open"), "folder-open");
    assert_eq!(row.key_text("default", true), "");
    assert_eq!(
        row.clone().with_key_hint("enter").key_text("default", true),
        "enter"
    );
    assert_eq!(row.with_key_hint("1").key_text("default", true), "1");
}
