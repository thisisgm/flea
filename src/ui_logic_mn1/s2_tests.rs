//! S2 freeze-gap contracts, with v0.3.12 as the behavior and wording oracle.
use super::{build::listing_entries, refresh::Refusal, testkit::*};

#[test]
fn s2_ey575_stale_menu_names_the_missing_item() {
    assert_eq!(
        english().tr(Refusal::Moved.key()),
        "Selected item moved or disappeared; reopen the menu."
    );
}

#[test]
fn s2_ey581_empty_background_keeps_enabled_invert_selection() {
    let mut context = state();
    context.has_row = false;
    context.selection_count = 0;
    context.hidden.clear();
    let rows = listing_entries(&context);
    let invert =
        entry(&rows, "invertSelection").expect("empty folders still offer Invert Selection");
    assert!(!invert.disabled);
    context.hidden.push("invertSelection".into());
    assert!(entry(&listing_entries(&context), "invertSelection").is_none());
}

#[test]
fn s2_neighbour_file_single_click_does_nothing() {
    let binding = crate::keys::pointer("neighbour", "file", "left", 0, 1).unwrap();
    assert_eq!(binding.effect, "nothing");
    assert_eq!(
        crate::keys::pointer("neighbour", "dir", "left", 0, 1)
            .unwrap()
            .effect,
        "reveal"
    );
    assert_eq!(
        crate::keys::pointer("neighbour", "file", "left", 0, 2)
            .unwrap()
            .effect,
        "open"
    );
}

#[test]
fn s2_manual_hold_hint_matches_the_shipped_words() {
    assert_eq!(
        english().tr_args("preview.loadHint", &["Ctrl+Space"]),
        "Ctrl+Space loads it"
    );
}
