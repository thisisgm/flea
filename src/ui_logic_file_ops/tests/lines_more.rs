// Ported from tests/js/ops.js, continued: the claim, rename, trash and bulk lines.
use super::lines::{card, item};
use crate::ui_logic_file_ops::lines::{self};
use crate::ui_logic_file_ops::testcat::Catalog;
use crate::ui_logic_file_ops::transfer::Transfer;

#[test]
fn rename_changed_text_reports_a_changed_item() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:316
    assert_eq!(
        lines::rename_changed(&cat.i18n),
        "Item changed; reopen Rename."
    );
}

#[test]
fn rename_refuses_every_invalid_basename() {
    let cat = Catalog::english();
    for invalid in ["", ".", "..", "a/b", "a\u{0}b"] {
        // oracle tests/js/ops.js:321
        assert!(
            lines::rename_refusal(&cat.i18n, invalid).is_some_and(|text| !text.is_empty()),
            "refused {invalid:?}"
        );
    }
}

#[test]
fn menu_rename_route_is_a_pure_function() {
    // oracle tests/js/ops.js:333
    let _route: fn(bool, &str, &str) -> &'static str = lines::menu_rename_route;
}

#[test]
fn menu_rename_route_ready_snapshot_opens_now() {
    // oracle tests/js/ops.js:335
    assert_eq!(lines::menu_rename_route(true, "sel-3", "sel-3"), "now");
}

#[test]
fn menu_rename_route_in_flight_takes_f2() {
    // oracle tests/js/ops.js:337
    assert_eq!(lines::menu_rename_route(false, "sel-3", "sel-3"), "f2");
}

#[test]
fn menu_rename_route_refuses_a_moved_ready_selection() {
    // oracle tests/js/ops.js:339
    assert_eq!(lines::menu_rename_route(true, "sel-3", "sel-4"), "stale");
}

#[test]
fn menu_rename_route_refuses_a_moved_in_flight_selection() {
    // oracle tests/js/ops.js:341
    assert_eq!(lines::menu_rename_route(false, "sel-3", "sel-4"), "stale");
}

#[test]
fn menu_rename_route_refuses_an_empty_identity() {
    // oracle tests/js/ops.js:343
    assert_eq!(lines::menu_rename_route(true, "", ""), "stale");
}

#[test]
fn refuse_rename_route_is_a_pure_function() {
    // oracle tests/js/ops.js:350
    let _route: fn(bool, bool) -> &'static str = lines::refuse_rename_route;
}

#[test]
fn refuse_rename_shows_in_a_live_editor() {
    // oracle tests/js/ops.js:358
    assert_eq!(lines::refuse_rename_route(true, true), "editor");
}

#[test]
fn refuse_rename_goes_to_status_without_an_editor() {
    // oracle tests/js/ops.js:362
    assert_eq!(lines::refuse_rename_route(false, true), "status");
}

#[test]
fn refuse_rename_on_a_settled_edit_still_says_it_once() {
    // oracle tests/js/ops.js:367
    assert_eq!(lines::refuse_rename_route(false, false), "status");
}

#[test]
fn made_folder_names_its_reversal() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:408
    assert_eq!(
        lines::made(&cat.i18n, "default", "/d/New Folder"),
        "Created New Folder · z undoes"
    );
}

#[test]
fn transfer_headline_is_the_count_alone() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:416
    assert_eq!(card(false, 23, 8).head(&cat.i18n), "Copying 9 of 23");
    // oracle tests/js/ops.js:419
    assert_eq!(card(true, 23, 8).head(&cat.i18n), "Moving 9 of 23");
}

#[test]
fn status_line_is_the_headline_plus_name() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:423
    assert_eq!(
        lines::progress_line(&cat.i18n, &item(false, 23, 8, "panel-demo.mp4")),
        "Copying 9 of 23 · panel-demo.mp4"
    );
}

#[test]
fn file_line_names_the_file_and_its_size() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:427
    assert_eq!(
        Transfer {
            name: "panel-demo.mp4".into(),
            total: 48_000_000,
            ..Default::default()
        }
        .file_line(&cat.i18n),
        "panel-demo.mp4 · 48.0 MB"
    );
}

#[test]
fn directory_with_nothing_copied_claims_no_size() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:431
    assert_eq!(
        Transfer {
            name: "photos".into(),
            total: 0,
            bytes: 0,
            ..Default::default()
        }
        .file_line(&cat.i18n),
        "photos"
    );
}

#[test]
fn directory_under_way_still_claims_no_size() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:433
    assert_eq!(
        Transfer {
            name: "photos".into(),
            total: 0,
            bytes: 1_500_000_000,
            ..Default::default()
        }
        .file_line(&cat.i18n),
        "photos"
    );
}

#[test]
fn nothing_in_flight_draws_no_file_row() {
    let cat = Catalog::english();
    // oracle tests/js/ops.js:434
    assert_eq!(
        Transfer {
            name: String::new(),
            total: 0,
            bytes: 0,
            ..Default::default()
        }
        .file_line(&cat.i18n),
        ""
    );
}

#[test]
fn one_file_half_copied_fills_half_the_bar() {
    // oracle tests/js/ops.js:439
    assert_eq!(
        Transfer {
            n: 1,
            done: 0,
            bytes: 24_000_000,
            total: 48_000_000,
            ..Default::default()
        }
        .fraction(),
        0.5
    );
}

#[test]
fn eight_of_twenty_three_sits_eight_steps_along() {
    // oracle tests/js/ops.js:443
    assert_eq!(
        Transfer {
            n: 23,
            done: 8,
            ..Default::default()
        }
        .fraction(),
        8.0 / 23.0
    );
}

#[test]
fn byte_sample_fills_only_the_item_in_flight() {
    // oracle tests/js/ops.js:446
    assert_eq!(
        Transfer {
            n: 23,
            done: 8,
            bytes: 24_000_000,
            total: 48_000_000,
            ..Default::default()
        }
        .fraction(),
        8.5 / 23.0
    );
}

#[test]
fn empty_transfer_reports_no_progress() {
    // oracle tests/js/ops.js:449
    assert_eq!(
        Transfer {
            n: 0,
            done: 0,
            bytes: 0,
            total: 0,
            ..Default::default()
        }
        .fraction(),
        0.0
    );
}

#[test]
fn bar_cannot_overfill() {
    // oracle tests/js/ops.js:453
    assert_eq!(
        Transfer {
            n: 1,
            done: 0,
            bytes: 99,
            total: 10,
            ..Default::default()
        }
        .fraction(),
        1.0
    );
}
