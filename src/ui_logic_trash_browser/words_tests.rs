//! tests/js/trash.js: the dd pair, routing, the delete card's words, the m target and the backend's order pins.
use super::dates_tests::english;
use super::words::{self, confirm_body, confirm_title, menu_target, route, Arm, MenuTarget, Route, Scope, Snapshot, ARM_MS};

#[test]
fn trash_pair_arms_then_trashes_inside_the_window() {
    let i18n = english();
    let (arm_key, alone) = words::prompt_keys("default");
    let mut arm = Arm::default();
    assert!(!arm.press(1_000), "the first d trashes nothing");
    assert_eq!(words::trash_prompt(&i18n, words::targets(&[], 3).len(), &arm_key, &alone),
        "Press d again to move it to Trash, or press Delete on its own.", "and the sentence names both routes");
    assert!(arm.armed(), "and it leaves the pair armed");
    assert!(arm.press(1_000 + ARM_MS - 1), "the second d inside the window completes the pair");
    assert_eq!(words::targets(&[], 3), vec![3], "and trashes the cursor row");
    assert!(!arm.armed(), "and disarms, so a third d only arms again");
    assert_eq!(words::trash_prompt(&i18n, words::targets(&[2], 3).len(), &arm_key, &alone),
        "Press d again to move it to Trash, or press Delete on its own.", "one marked row is still it");
    assert_eq!(words::trash_prompt(&i18n, words::targets(&[1, 4], 3).len(), &arm_key, &alone),
        "Press d again to move them to Trash, or press Delete on its own.", "two marked rows are them");
    assert_eq!(words::trash_prompt(&i18n, 1, &arm_key, ""), "Press d again to move it to Trash.",
        "with no key of its own the single-row sentence ends at Trash");
    assert_eq!(words::trash_prompt(&i18n, 2, &arm_key, ""), "Press d again to move them to Trash.", "and the plural one does too");
    assert!(!alone.is_empty(), "Delete is bound in the shared table");
    assert_eq!(words::review_prompt(&i18n, &arm_key, &alone), "Press d again to review permanent deletion, or Delete on its own.");
    assert_eq!(words::review_prompt(&i18n, &arm_key, ""), "Press d again to review permanent deletion.");
    let mut stale = Arm::default();
    stale.press(0);
    assert!(!stale.press(60_000), "a d a minute after the first is a fresh arm, not the second of a pair");
    assert!(stale.armed(), "and it re-arms rather than completing a pair nobody meant");
    assert!(!stale.press(60_000 + ARM_MS) && stale.armed(), "the window is exclusive at its edge");
    let mut cleared = Arm::default();
    cleared.press(0);
    cleared.clear();
    assert!(!cleared.press(10), "any other key clears the arm, so the next d arms again");
    assert_eq!(words::targets(&[1, 4], 3), vec![1, 4], "the pair trashes the selection when there is one");
}

#[test]
fn trash_routes_window_keys_and_keeps_listing_keys() {
    for action in ["keymapSheet", "pathBar", "textSizeUp", "textSizeDown", "textSizeReset"] {
        assert_eq!(route(action), Route::Direct, "{action} reaches the window directly");
    }
    for action in ["settings", "sidebar", "tabNew", "tabClose", "tab3"] {
        assert_eq!(route(action), Route::Act, "{action} goes through the pane");
    }
    for action in ["cursorDown", "trashArm", "menu", "search", "filter", "openTerminal", "copydirpath"] {
        assert_eq!(route(action), Route::Trash, "{action} stays in Trash");
    }
}

#[test]
fn trash_card_words_are_whole_or_absent() {
    let i18n = english();
    for unfilled in [Snapshot::default(), Snapshot { all: true, ..Snapshot::default() }, Snapshot { token: 1, ..Snapshot::default() }] {
        assert_eq!(format!("{}|{}", confirm_title(&i18n, &unfilled), confirm_body(&i18n, &unfilled, Scope::Trash)), "|",
            "an unfilled snapshot {unfilled:?} draws no title and no body");
    }
    let count = |n| Snapshot { count: Some(n), ..Snapshot::default() };
    assert_eq!(format!("{} // {}", confirm_title(&i18n, &count(1)), confirm_title(&i18n, &count(1234))),
        "Delete 1 item permanently? // Delete 1,234 items permanently?", "singular and plural");
    let all = Snapshot { all: true, count: Some(3), bytes: 4096, token: 0 };
    assert_eq!(format!("{} // {}", confirm_title(&i18n, &all), confirm_body(&i18n, &all, Scope::Trash)),
        "Empty Trash? // 3 items, 4.1 kB. This deletes them from disk. This cannot be undone.");
    let one = Snapshot { count: Some(1), ..all.clone() };
    assert_eq!(confirm_body(&i18n, &one, Scope::Trash), "1 item, 4.1 kB. This deletes it from disk. This cannot be undone.");
    assert_eq!(confirm_body(&i18n, &count(2), Scope::Items), "These items are deleted from disk. This cannot be undone.");
    assert_eq!(confirm_body(&i18n, &count(1), Scope::Trash), "This Trash item is deleted from disk. This cannot be undone.");
}

#[test]
fn trash_menu_target_follows_marks_and_cursor() {
    assert_eq!(menu_target(0, 0, false), MenuTarget::Strip, "an empty Trash keeps the whole-Trash menu");
    assert_eq!(menu_target(5, 0, false), MenuTarget::Cursor, "nothing marked opens the cursor row's menu");
    assert_eq!(menu_target(5, 1, true), MenuTarget::Cursor, "a lone mark on the cursor row is still the cursor row's");
    assert_eq!(menu_target(5, 1, false), MenuTarget::Marked, "a lone mark off the cursor row is the marked rows'");
    assert_eq!(menu_target(5, 2, true), MenuTarget::Marked, "two marked rows open the marked rows' menu");
    assert_eq!(menu_target(5000, 5000, true), MenuTarget::Marked, "a bulk selection opens the marked rows' menu");
    assert_eq!(menu_target(1, 0, false), MenuTarget::Cursor, "one item with nothing marked is the cursor row's menu");
}

#[test]
fn trash_backend_order_and_membership_pins_hold() {
    let rust = include_str!("../backend/trashbrowse.rs");
    assert!(rust.contains("dated.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.uri.cmp(&b.1.uri)))"), "newest first, then the uri");
    assert!(rust.contains(r#"read(&["list", "-u", "-h", "-a", "trash::deletion-date", "trash:///"])"#) && !rust.contains("fn deletion_date("),
        "one gio list dates every volume's items");
    assert!(rust.contains("newest_first(&mut items, |item| dates.get(&item.uri)")
        && rust.contains("fn list() -> Result<Vec<Item>, Refusal> {\n    list_with(gio)\n}"), "one reader orders every listing");
    assert!(rust.contains("let listed = listed_uris(&self.items);") && rust.contains("if !listed.contains(item.uri.as_str()) { continue; }")
        && !rust.contains("binary_search"), "membership is by uri set");
    assert!(rust.contains("if !same_items(&list()?, &self.items) {") && rust.contains("if !same_items(&list()?, &confirmation.listing) {")
        && !rust.contains("list()? != "), "staleness ignores the order of the two reads");
}
