//! The status bar's priority composition, board strings first.
use super::english;
use flea_gui::strips::status_host::{self, CentreRole, Listing, Slot};

fn slot(transient: &str, is_error: bool) -> Slot {
    Slot {
        transient: transient.into(),
        transient_is_error: is_error,
        ..Slot::default()
    }
}

#[test]
fn an_empty_folder_counts_zero_items() {
    let i18n = english();
    assert_eq!(status_host::item_text(&i18n, Listing::Empty), "0 items");
    assert_eq!(status_host::item_text(&i18n, Listing::Ready(9)), "9 items");
    assert_eq!(status_host::item_text(&i18n, Listing::Ready(1)), "1 item");
    assert_eq!(status_host::item_text(&i18n, Listing::Error), "unavailable");
    assert_eq!(
        status_host::item_text(&i18n, Listing::Locked),
        "unavailable"
    );
    assert_eq!(status_host::item_text(&i18n, Listing::Loading), "");
}

#[test]
fn the_left_zone_prefers_flash_then_selection_then_filter() {
    let i18n = english();
    let ready = Listing::Ready(80);
    assert_eq!(
        status_host::left(
            &i18n,
            ready,
            "Drives",
            Some((10, 70, "70 B".into())),
            Some("10 of 80 shown")
        ),
        "Drives"
    );
    assert_eq!(
        status_host::left(
            &i18n,
            ready,
            "",
            Some((10, 70, "70 B".into())),
            Some("10 of 80 shown")
        ),
        "10 of 80 selected · 70 B"
    );
    assert_eq!(
        status_host::left(
            &i18n,
            ready,
            "",
            Some((0, -1, String::new())),
            Some("1 of 9 shown")
        ),
        "1 of 9 shown"
    );
    assert_eq!(status_host::left(&i18n, ready, "", None, None), "80 items");
}

#[test]
fn a_listing_that_is_not_ready_answers_with_its_own_count() {
    let i18n = english();
    let selection = Some((10, 70, "70 B".to_string()));
    // StatusBar.qml answers a listing that is not ready before the flash, selection or filter sentence can stand.
    assert_eq!(
        status_host::left(
            &i18n,
            Listing::Loading,
            "Drives",
            selection.clone(),
            Some("10 of 80 shown")
        ),
        ""
    );
    assert_eq!(
        status_host::left(
            &i18n,
            Listing::Error,
            "Drives",
            selection.clone(),
            Some("10 of 80 shown")
        ),
        "unavailable"
    );
    assert_eq!(
        status_host::left(
            &i18n,
            Listing::Empty,
            "Drives",
            selection,
            Some("10 of 80 shown")
        ),
        "0 items"
    );
}

#[test]
fn a_listing_that_is_not_ready_answers_the_directory_and_nothing_else() {
    let i18n = english();
    // StatusBar.qml's countText returns itemText before it looks at the flasher, the selection or the filter's sentence.
    for listing in [
        Listing::Loading,
        Listing::Error,
        Listing::Locked,
        Listing::Empty,
    ] {
        assert_eq!(
            status_host::left(
                &i18n,
                listing,
                "Drives",
                Some((10, 70, "70 B".into())),
                Some("10 of 80 shown")
            ),
            status_host::item_text(&i18n, listing)
        );
    }
}

#[test]
fn a_selection_without_known_bytes_names_no_size() {
    let i18n = english();
    assert_eq!(
        status_host::selection_head(&i18n, 3, 9, -1, ""),
        "3 of 9 selected"
    );
    assert_eq!(
        status_host::selection_head(&i18n, 3, 9, 609, "609 B"),
        "3 of 9 selected · 609 B"
    );
}

#[test]
fn the_centre_keeps_prompts_and_errors_over_activity_and_search() {
    let error = slot("Copy failed", true);
    assert_eq!(status_host::centre(&error), "Copy failed");
    assert_eq!(status_host::centre_role(&error), CentreRole::Error);
    let prompt = Slot {
        prompt_live: true,
        transient: "Empty Trash…".into(),
        ..Slot::default()
    };
    assert_eq!(status_host::centre(&prompt), "Empty Trash…");
    assert_eq!(status_host::centre_role(&prompt), CentreRole::Foreground);
    let sticky = Slot {
        sticky: "Copying 2 of 5".into(),
        transient: "Done".into(),
        ..Slot::default()
    };
    assert_eq!(status_host::centre(&sticky), "Copying 2 of 5");
    let searching = Slot {
        searching: true,
        search_line: "3 found".into(),
        ..Slot::default()
    };
    assert_eq!(status_host::centre(&searching), "3 found");
    let notice = slot("Reloaded · 1 item changed", false);
    assert_eq!(status_host::centre(&notice), "Reloaded · 1 item changed");
    assert_eq!(status_host::centre_role(&notice), CentreRole::Foreground);
}

#[test]
fn the_primary_lane_splits_the_hint_off() {
    assert_eq!(
        status_host::primary("Copied 2 items · z undoes", " · z undoes"),
        "Copied 2 items"
    );
    assert_eq!(status_host::primary("Copied 2 items", ""), "Copied 2 items");
    assert_eq!(
        status_host::primary("Copied 2 items", " · p pastes"),
        "Copied 2 items"
    );
}

#[test]
fn a_notice_lives_four_seconds_and_then_needs_no_timer() {
    let notice = slot("Reloaded · 1 item changed", false);
    let shown_ms = status_host::shown_since(0, false, true, 0, true);
    let shown_ms = status_host::shown_since(2_000, true, true, shown_ms, false);
    assert_eq!(shown_ms, 0, "a notice that stays shown keeps its stamp");
    assert!(status_host::notice_live(3_999, shown_ms, false));
    assert!(!status_host::notice_live(4_000, shown_ms, false));
    assert!(status_host::notice_live(4_000, 1, false));
    assert!(status_host::notice_live(3_999, 0, false));
    assert!(!status_host::notice_live(4_000, 0, false));
    assert!(status_host::needs_timer(&notice, false));
    assert!(!status_host::needs_timer(&slot("", false), false));
}

#[test]
fn a_new_notice_said_over_a_shown_one_gets_its_full_time() {
    // StatusBar.qml say() restarts the clear timer: B said at 3500 stands at 7000, and undo stands at any age until replaced.
    let shown_a = status_host::shown_since(0, false, true, 0, true);
    assert!(status_host::notice_live(3_499, shown_a, false));
    let shown_b = status_host::shown_since(3_500, true, true, shown_a, true);
    assert_eq!(shown_b, 3_500);
    assert!(status_host::notice_live(7_000, shown_b, false));
    assert!(!status_host::notice_live(7_500, shown_b, false));
    assert!(status_host::notice_live(5_000, 5_000, true));
    assert!(status_host::notice_live(9_000, 5_000, true));
}

#[test]
fn an_undo_notice_runs_no_clear_timer() {
    // StatusBar.qml syncNoticeTimer: " · z undoes" stays until replaced, so its click target stays.
    assert!(!status_host::needs_timer(
        &slot("Copied 2 items · z undoes", false),
        true
    ));
    assert!(status_host::needs_timer(
        &slot("Copied 2 items", false),
        false
    ));
}

#[test]
fn an_error_activity_prompt_or_search_covers_the_notice_and_stops_the_timer() {
    // Status.js noticeShown is false while any of these stands, and StatusBar.qml stops the clear timer with it.
    for covered in [
        slot("Copy failed", true),
        Slot {
            transient: "Reloaded · 1 item changed".into(),
            prompt_live: true,
            ..Slot::default()
        },
        Slot {
            transient: "Reloaded · 1 item changed".into(),
            sticky: "Copying 2 of 5".into(),
            ..Slot::default()
        },
        Slot {
            transient: "Reloaded · 1 item changed".into(),
            searching: true,
            ..Slot::default()
        },
    ] {
        assert!(!status_host::notice_shown(&covered));
        assert!(!status_host::needs_timer(&covered, false));
    }
}

#[test]
fn a_hidden_notice_keeps_its_full_time_when_shown_again() {
    // StatusBar.qml clear.restart(): a notice shown at 0, covered at 1000, gets its full 4000 ms once shown again at 5000.
    let mut shown_ms = status_host::shown_since(0, false, true, 0, true);
    assert!(!status_host::notice_shown(&slot("Copy failed", true)));
    shown_ms = status_host::shown_since(1_000, true, false, shown_ms, false);
    assert_eq!(
        shown_ms, 0,
        "a covered notice keeps its stamp while the error stands"
    );
    shown_ms = status_host::shown_since(5_000, false, true, shown_ms, false);
    assert_eq!(shown_ms, 5_000);
    assert!(status_host::notice_live(8_999, shown_ms, false));
    assert!(!status_host::notice_live(9_000, shown_ms, false));
}

#[test]
fn the_disk_facts_keep_their_zone() {
    let i18n = english();
    // StatusBar.qml fsText: a known name wins even while the next answer is out (Nav.js keeps fsName).
    assert_eq!(
        status_host::disk(&i18n, "tmpfs", Some("64.1 MB"), true),
        "tmpfs · 64.1 MB free"
    );
    assert_eq!(
        status_host::disk(&i18n, "tmpfs", Some("64.1 MB"), false),
        "tmpfs · 64.1 MB free"
    );
    assert_eq!(status_host::disk(&i18n, "tmpfs", None, false), "unknown");
    assert_eq!(status_host::disk(&i18n, "", None, false), "unknown");
    assert_eq!(status_host::disk(&i18n, "", None, true), "");
}

#[test]
fn the_reload_notice_counts_what_the_watch_reported() {
    let i18n = english();
    assert_eq!(status_host::reloaded(&i18n, 1), "Reloaded · 1 item changed");
    assert_eq!(
        status_host::reloaded(&i18n, 6),
        "Reloaded · 6 items changed"
    );
}

#[test]
fn selection_bytes_need_every_row_held_with_a_known_size() {
    assert_eq!(status_host::selection_bytes(0, 9, &[], &[], &[]), -1);
    assert_eq!(status_host::selection_bytes(10, 9, &[], &[], &[]), -1);
    assert_eq!(
        status_host::selection_bytes(2, 9, &[Some(100), Some(47)], &[true, true], &[false, false]),
        147
    );
    assert_eq!(
        status_host::selection_bytes(2, 9, &[Some(100), None], &[true, true], &[false, false]),
        -1
    );
    assert_eq!(
        status_host::selection_bytes(2, 9, &[Some(100), Some(47)], &[true, false], &[false, true]),
        -1
    );
}
