//! The keyboard's search mounted in the whole window: key bindings only, Ctrl+F and Escape at every size, nothing run unpicked.
use super::scroll_tests::draw;
use super::sheet_keyboard_mounted_tests::{open, opened, Open};
use flea::ui_logic_st1::{sheet::MenuEntry, Text};

#[test]
fn the_sheet_finds_key_bindings_only_wherever_a_keyboard_is_known() {
    // "permis" finds the Permissions menu row in the command palette and no key binding.
    let found = |width: f32, height: f32, keyboard: bool| {
        let mut sheet = opened(width, height);
        sheet.view.update(&mut sheet.cx, |view, _| {
            view.sheet.sources.menus = vec![MenuEntry {
                label: Text::data("Permissions…"),
                action: "permissions".into(),
                ..Default::default()
            }];
        });
        if !keyboard {
            sheet
                .view
                .update(&mut sheet.cx, |view, _| view.sheet.set_keyboard(None));
            draw(&mut sheet.cx);
        } else if width > 1000. {
            sheet.cx.simulate_keystrokes("ctrl-f");
        }
        sheet.cx.simulate_keystrokes("p e r m i s");
        draw(&mut sheet.cx);
        sheet
            .view
            .read_with(&sheet.cx, |view, _| view.sheet.ranked.len())
    };
    assert!(
        found(2560., 1400., false) > 0,
        "the palette finds the menu row"
    );
    assert_eq!(
        found(2560., 1400., true),
        0,
        "the keyboard's search does not"
    );
}

/// Whether the keyboard's search is open, and whether the sheet is.
fn searching(sheet: &mut Open) -> (bool, bool) {
    let typing = sheet.view.read_with(&sheet.cx, |view, _| view.sheet.typing);
    (typing, open(sheet))
}

#[test]
fn a_window_too_small_for_the_keyboard_only_asks_for_a_larger_one() {
    let mut sheet = opened(700., 480.);
    sheet.cx.simulate_keystrokes("ctrl-f r e n ctrl-c");
    let state = sheet.view.read_with(&sheet.cx, |view, _| {
        (
            view.sheet.typing,
            view.sheet.query.clone(),
            view.sheet.chord,
        )
    });
    assert_eq!(
        state,
        (false, String::new(), None),
        "keys neither search nor look up"
    );
    assert!(open(&mut sheet));
    sheet.cx.simulate_keystrokes("escape");
    assert!(!open(&mut sheet), "Escape closes it");
}

#[test]
fn a_search_shrunk_below_the_keyboard_closes_with_escape() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f");
    sheet
        .cx
        .simulate_resize(gpui::size(gpui::px(700.), gpui::px(480.)));
    draw(&mut sheet.cx);
    sheet.cx.simulate_keystrokes("escape");
    assert!(!open(&mut sheet));
}

#[test]
fn the_close_mark_closes_the_sheet() {
    for (width, height) in [(2560., 1400.), (700., 480.)] {
        let mut sheet = opened(width, height);
        let close = sheet.cx.debug_bounds("sheet-close").expect("a close mark");
        sheet
            .cx
            .simulate_click(close.center(), gpui::Modifiers::none());
        assert!(!open(&mut sheet), "{width}x{height}");
    }
}

#[test]
fn the_open_search_runs_nothing_until_a_row_is_picked_and_page_keys_keep_it_open() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f enter pagedown tab f5");
    assert_eq!(searching(&mut sheet), (true, true));
    let decided = sheet
        .view
        .read_with(&sheet.cx, |view, _| view.sheet.decided.clone());
    assert_eq!(decided, None, "Enter before a row is picked runs nothing");
    let chord = sheet.view.read_with(&sheet.cx, |view, _| view.sheet.chord);
    assert_eq!(chord, None, "F5 mid-search is not looked up");
    sheet.cx.simulate_keystrokes("down");
    let cursor = sheet.view.read_with(&sheet.cx, |view, _| {
        (view.sheet.cursor, view.sheet.chosen())
    });
    assert_eq!(cursor, (0, true), "the first Down picks the first row");
    sheet.cx.simulate_keystrokes("enter");
    let decided = sheet
        .view
        .read_with(&sheet.cx, |view, _| view.sheet.decided.clone());
    assert_eq!(decided, None, "a picked row is shown, never run");
    assert!(open(&mut sheet));
}

#[test]
fn a_dead_key_composes_nothing_until_the_search_is_open() {
    let mut sheet = opened(2560., 1400.);
    let composed = |sheet: &mut Open| {
        super::ime_tests::platform(&mut sheet.cx, |input| {
            input.replace_and_mark_text_in_range(None, "´", None);
            input.replace_text_in_range(None, "é");
        });
        draw(&mut sheet.cx);
        sheet
            .view
            .read_with(&sheet.cx, |view, _| view.sheet.query.clone())
    };
    assert_eq!(
        composed(&mut sheet),
        "",
        "at rest keys only look themselves up"
    );
    sheet.cx.simulate_keystrokes("ctrl-f");
    draw(&mut sheet.cx);
    assert_eq!(
        composed(&mut sheet),
        "é",
        "the open search composes as it always did"
    );
}

#[test]
fn the_keyboards_results_read_under_the_rest_lists_group_headings() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f");
    sheet.cx.simulate_keystrokes("p a");
    draw(&mut sheet.cx);
    let (groups, heads, scroll, count) = sheet.view.update(&mut sheet.cx, |view, _| {
        let sheet = &mut view.sheet;
        let group = |row: &flea::ui_logic_st1::sheet::Candidate| {
            flea::ui_logic_st1::sheet_keyboard::group_of(&row.action)
        };
        let groups: Vec<_> = sheet.ranked.iter().map(group).collect();
        let heads: Vec<_> = (0..sheet.ranked.len())
            .filter_map(|index| sheet.result_head(index))
            .collect();
        let count = sheet.ranked.len();
        sheet.point(count - 1, false);
        sheet.reveal(250., 25.);
        (groups, heads, sheet.scroll, count)
    });
    let mut distinct = groups.clone();
    distinct.dedup();
    let mut unique = distinct.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        distinct.len(),
        unique.len(),
        "each group's rows together: {groups:?}"
    );
    assert!(
        distinct.len() > 1,
        "\"pa\" finds rows in more than one group: {groups:?}"
    );
    assert_eq!(heads, distinct, "one heading per group, over its first row");
    // The last row is revealed below every heading above it.
    let headings = heads.len() as f32 * super::sheet_view::HEAD_SLOT;
    assert_eq!(scroll, count as f32 * 25. + headings - 250.);
    sheet
        .view
        .update(&mut sheet.cx, |view, _| view.sheet.set_keyboard(None));
    let head = sheet
        .view
        .read_with(&sheet.cx, |view, _| view.sheet.result_head(0));
    assert_eq!(
        head, None,
        "the palette without a keyboard draws no headings"
    );
}

#[test]
fn the_best_match_leads_and_keys_the_rest_list_does_not_name_are_found_under_other() {
    let mut sheet = opened(2560., 1400.);
    sheet.cx.simulate_keystrokes("ctrl-f");
    sheet.cx.simulate_keystrokes("c u");
    draw(&mut sheet.cx);
    let first = |sheet: &mut Open| {
        sheet
            .view
            .read_with(&sheet.cx, |view, _| view.sheet.ranked[0].action.clone())
    };
    let grouped = first(&mut sheet);
    sheet
        .view
        .update(&mut sheet.cx, |view, _| view.sheet.set_keyboard(None));
    assert_eq!(
        grouped,
        first(&mut sheet),
        "the grouping keeps the ranker's best match first"
    );
    let mut sheet = opened(2560., 1400.);
    sheet.view.update(&mut sheet.cx, |view, _| {
        let keyboard = view.sheet.keyboard.as_mut().expect("a keyboard");
        let cap = keyboard.cap_of("j", Some("j")).expect("a J cap");
        keyboard
            .bound
            .push(flea::ui_logic_st1::sheet_keyboard::Bound {
                cap,
                mask: flea::keys::CONTROL,
                action: "je93.checksums:sha256".into(),
                label: "Verify checksum".into(),
            });
    });
    sheet.cx.simulate_keystrokes("ctrl-f");
    sheet.cx.simulate_keystrokes("v e r i f");
    draw(&mut sheet.cx);
    let found = sheet.view.read_with(&sheet.cx, |view, _| {
        let sheet = &view.sheet;
        let at = sheet
            .ranked
            .iter()
            .position(|row| row.action == "je93.checksums:sha256");
        at.map(|at| (sheet.result_keys(at).to_owned(), sheet.result_head(at)))
    });
    assert_eq!(
        found,
        Some(("CTRL + J".to_owned(), Some(None))),
        "a plugin's key under OTHER"
    );
}

#[test]
fn over_a_keyboard_every_list_spells_its_keys_as_the_caps_read() {
    let mut sheet = opened(2560., 1400.);
    let copy = |sheet: &mut Open| {
        sheet.view.read_with(&sheet.cx, |view, _| {
            let row = view.sheet.rows.iter().find(|row| row.action == "copy");
            view.sheet.cap(&row.expect("a Copy row").keys).to_owned()
        })
    };
    assert_eq!(copy(&mut sheet), "Y / CTRL + C");
    sheet
        .view
        .update(&mut sheet.cx, |view, _| view.sheet.set_keyboard(None));
    assert_eq!(
        copy(&mut sheet),
        "y / ctrl-c",
        "without a keyboard the sheet spells as it always did"
    );
}
