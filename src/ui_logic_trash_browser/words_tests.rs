//! tests/js/trash.js: the dd pair, routing, the delete card's words, the m target and the backend's order pins.
use super::dates_tests::english;
use super::words::{
    self, confirm_body, confirm_title, menu_target, route, Arm, MenuTarget, Route, Scope, Snapshot,
    ARM_MS,
};

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
    let tokens = rust
        .split_whitespace()
        .collect::<String>()
        .replace(",]", "]");
    let metadata = include_str!("../backend/trashbrowse/topdir.rs")
        .split_whitespace()
        .collect::<String>()
        .replace(",]", "]");
    assert!(
        rust.contains("dated.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.uri.cmp(&b.1.uri)))"),
        "newest first, then the uri"
    );
    assert!(
        metadata.contains(r#"read(&["list","-u","-h","-a","standard::target-uri,trash::deletion-date","trash:///"])"#)
            && !rust.contains("fn deletion_date("),
        "one gio list dates every volume's items"
    );
    assert!(
        tokens.contains("newest_first(&mutitems,|item|{dates.get(&item.uri)")
            && tokens.contains("fnlist()->Result<Vec<Item>,Refusal>{list_with(gio)}"),
        "one reader orders every listing"
    );
    assert!(
        rust.contains("let listed = listed_uris(&self.items);")
            && rust.contains("if !listed.contains(item.uri.as_str()) { continue; }")
            && !rust.contains("binary_search"),
        "membership is by uri set"
    );
    assert!(
        rust.contains("if !same_items(&list()?, &self.items) {")
            && rust.contains("if !same_items(&list()?, &confirmation.listing) {")
            && !rust.contains("list()? != "),
        "staleness ignores the order of the two reads"
    );
}

#[test]
fn trash_leftover_footer_is_filled_and_formats_in_all_shipped_locales() {
    use crate::i18n::{I18n, Paths};

    let root = crate::backend::testdir::TestDir::new("trash-leftover-locales");
    let roster = root.dir("omarchy/default/i18n");
    std::fs::write(
        roster.join("locales.json"),
        include_str!("../i18n/fixtures/locales.json"),
    )
    .unwrap();
    let catalogs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n");
    let mut checked = 0;
    for entry in std::fs::read_dir(&catalogs).unwrap().flatten() {
        let catalog = entry.path().join("flea.json");
        if !catalog.is_file() {
            continue;
        }
        let code = entry.file_name().to_str().unwrap().to_string();
        let document = crate::jsondoc::parse(&std::fs::read_to_string(catalog).unwrap()).unwrap();
        let forms = document.get("trash.done.leftover").unwrap();
        let language_file = root.file("language", &code);
        let i18n = I18n::load(Paths {
            omarchy: root.join("omarchy"),
            language_file,
            catalogs: vec![catalogs.clone()],
        });
        assert_eq!(i18n.locale().code, code, "the catalog's locale is selected");
        let headline = words::done_line(&i18n, false, 2, 0);
        assert_eq!(words::leftover_line(&i18n, headline.clone(), 0), headline);
        for count in [1, 2, 3, 5, 11, 21, 100, 1_000_000] {
            let category = crate::i18n::plural::category(&code, count);
            let form = forms
                .get(category)
                .and_then(crate::jsondoc::Json::as_str)
                .unwrap();
            assert!(
                form.contains("%1") && form.contains("%2"),
                "{code}/{category}"
            );
            let footer = words::leftover_line(&i18n, headline.clone(), count as usize);
            assert!(footer.contains(&headline), "{code}/{count}: {footer}");
            assert!(
                footer.contains(&i18n.locale().format_count(count)),
                "{code}/{count}: {footer}"
            );
            assert!(!footer.contains('%'), "{code}/{count}: {footer}");
            if code != "en" {
                assert!(
                    !footer.contains("leftover entr"),
                    "{code}/{count}: {footer}"
                );
            }
            assert!(
                !footer.contains("trash.done.leftover"),
                "{code}/{count}: {footer}"
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 50, "every shipped catalog exercises the footer");
}
