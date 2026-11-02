//! EY-515: the several-item Permissions card always says how mixed boxes behave, as PermissionsDialog.qml's scopeLabel does.
use super::permissions::Permissions;
use super::shell::Ink;
use super::tests::{english, notes};
use flea::jsondoc::parse;
use flea::ui_logic_op3::permissions::INSPECT_STRIDE;

const CARD_ID: u64 = 5;
const MIXED_NOTE: &str = "Mixed boxes keep each item's setting unless you change them.";

fn two_items(modes: [&str; 2]) -> Permissions {
    let i18n = english();
    let paths = vec!["/fx/v/work/notes.txt".into(), "/fx/v/work/todo.md".into()];
    let (mut card, _) = Permissions::open(CARD_ID, paths, "");
    for (at, mode) in modes.iter().enumerate() {
        card.reply(
            &i18n,
            &parse(&format!(
                r#"{{"op":"inspect","id":{},"ok":true,"mode":"{mode}","reason":""}}"#,
                CARD_ID * INSPECT_STRIDE + at as u64
            ))
            .unwrap(),
        );
    }
    card
}

#[test]
fn fixcards2_ey515_two_items_with_no_mixed_box_still_say_the_mixed_sentence() {
    let drawn = two_items(["0644", "0644"]).card(&english());
    assert_eq!(drawn.title, "Permissions for 2 Items");
    assert_eq!(notes(&drawn), vec![(MIXED_NOTE.into(), Ink::Foreground)]);
}

#[test]
fn fixcards2_ey515_two_items_with_a_mixed_box_say_it_once() {
    let drawn = two_items(["0644", "0755"]).card(&english());
    assert_eq!(notes(&drawn), vec![(MIXED_NOTE.into(), Ink::Foreground)]);
}
