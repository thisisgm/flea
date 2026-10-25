use super::*;
use flea_gui::pane::swap::{CAP_MS, MIN_SHOW_MS};

#[test]
fn fast_pair_lands_without_a_loading_mark_and_cap_includes_four_120hz_frames() {
    assert!(CAP_MS as f64 + 4. * 1000. / 120. <= 100.);
    assert_eq!(MIN_SHOW_MS, 100);
    for clear_at_once in [false, true] {
        let mut pane = settled();
        pane.begin(
            "/fixture/b".into(),
            Options {
                clear_at_once,
                ..Options::default()
            },
            1000,
        );
        header(&mut pane, "/fixture/b", 2, 1040);
        assert!(answer(&mut pane, 0, 2, 2, 1040));
        assert_eq!(pane.swap.counters.fallbacks, 0);
        assert_eq!(pane.swap.counters.shown_ms, 0);
    }
}

#[test]
fn slow_pairs_and_empty_pairs_keep_the_drawn_mark_for_100ms() {
    for delay in [90, 110, 140, 400, 2500] {
        for count in [0, 2] {
            let mut pane = settled();
            pane.begin("/fixture/b".into(), Options::default(), 1000);
            assert!(!pane.tick(1065));
            assert!(pane.tick(1066));
            pane.frame(1066, true);
            assert_eq!(pane.state.kind, Kind::Loading);
            header(&mut pane, "/fixture/b", count, 1000 + delay);
            let landed = answer(&mut pane, 0, count, 2, 1000 + delay);
            if delay < 166 {
                assert!(!landed);
                assert!(!pane.tick(1165));
                assert!(pane.tick(1166));
            } else {
                assert!(landed);
            }
            assert!(pane.swap.counters.shown_ms >= 100);
            assert_eq!(pane.swap.counters.fallbacks, 1);
            assert_eq!(pane.swap.counters.blank_frames, 0);
            assert_eq!(pane.swap.counters.loading_frames, 1);
            assert_eq!((pane.path(), pane.held.total), ("/fixture/b", count));
        }
    }
}

#[test]
fn a_header_kept_before_cap_stays_paired_after_cap() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    header(&mut pane, "/fixture/b", 3, 1040);
    assert!(pane.tick(1066));
    pane.frame(1066, true);
    assert!(!answer(&mut pane, 0, 3, 2, 1100));
    assert!(pane.tick(1166));
    assert_eq!((pane.path(), pane.held.total), ("/fixture/b", 3));
}

#[test]
fn duplicate_waiting_rows_cannot_replace_the_first_complete_pair() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    pane.tick(1066);
    pane.frame(1066, true);
    header(&mut pane, "/fixture/b", 3, 1100);
    assert!(!answer(&mut pane, 0, 3, 2, 1100));
    header(&mut pane, "/fixture/b", 99, 1101);
    assert!(!answer(&mut pane, 0, 99, 3, 1101));
    assert!(pane.tick(1166));
    assert_eq!((pane.held.total, pane.held.listing), (3, 2));
    assert_eq!(pane.swap.counters.waits, 1);
}

#[test]
fn an_overdue_timer_does_not_replace_a_complete_fast_answer() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    header(&mut pane, "/fixture/b", 2, 1110);
    assert!(answer(&mut pane, 0, 2, 2, 1110));
    assert!(!pane.tick(1111));
    assert_eq!(pane.swap.counters.fallbacks, 0);
}

#[test]
fn loading_animation_crawls_from_its_first_drawn_frame_except_under_reduced_motion() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    pane.tick(1066);
    assert!(!pane.swap.crawling(false));
    // The shell's render shows the player before the frame paints, so the first drawn frame already crawls.
    pane.swap.mark.show(true, gpui::Hsla::default());
    assert_eq!(pane.swap.mark_since, None);
    assert!(pane.swap.crawling(false));
    pane.frame(1100, true);
    assert_eq!(pane.swap.mark_since, Some(1100));
    assert!(pane.swap.crawling(false));
    assert!(!pane.swap.crawling(true));
    pane.swap.mark.show(false, gpui::Hsla::default());
    pane.swap.mark_shown(false, 9000);
    assert!(!pane.swap.crawling(false));
}

#[test]
fn the_minimum_starts_at_the_drawn_frame_including_time_zero() {
    let mut pane = Pane::new("/fixture/a".into(), 620.);
    pane.frame(0, true);
    assert_eq!(pane.swap.show_left(40), 60);
    pane.frame(20, true);
    assert_eq!(pane.swap.mark_since, Some(0));
    header(&mut pane, "/fixture/a", 2, 40);
    assert!(!answer(&mut pane, 0, 2, 1, 40));
    pane.tick(66);
    assert!(!pane.tick(99));
    assert!(pane.tick(100));
    assert_eq!(pane.swap.counters.shown_ms, 100);
}

#[test]
fn hiding_a_mark_with_a_waiting_pair_arms_an_immediate_commit() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    pane.tick(1066);
    pane.frame(1066, true);
    header(&mut pane, "/fixture/b", 2, 1100);
    assert!(!answer(&mut pane, 0, 2, 2, 1100));
    pane.swap.mark_shown(false, 1101);
    assert_eq!(pane.swap.deadline(1101), Some(std::time::Duration::ZERO));
    assert!(pane.tick(1101));
}

#[test]
fn an_explicit_anchor_window_waits_behind_the_first_pair_in_its_own_numbering() {
    let mut pane = settled();
    pane.begin("/fixture/b".into(), Options::default(), 1000);
    pane.tick(1066);
    pane.frame(1066, true);
    header(&mut pane, "/fixture/b", 1000, 1100);
    assert!(!answer(&mut pane, 0, 320, 2, 1100));
    let anchor = pane.anchor_window(680).unwrap();
    assert_eq!((anchor.start, anchor.count, anchor.listing), (680, 320, 2));
    assert!(pane.anchor_window(300).is_none());
    assert!(!answer(&mut pane, 680, 320, 3, 1110));
    assert!(!answer(&mut pane, 680, 320, 2, 1110));
    assert_eq!(pane.path(), "/fixture/a");
    assert!(pane.tick(1166));
    assert_eq!(
        (pane.path(), pane.held.start, pane.held.listing),
        ("/fixture/b", 680, 2)
    );
    assert_eq!(pane.swap.counters.commits, 2);
    assert_eq!(pane.swap.counters.waits, 2);
    assert!(pane.requests.outstanding.is_none());
}

#[test]
fn a_missing_restore_name_parks_the_first_pair_until_its_own_deadline() {
    let mut pane = settled();
    pane.begin(
        "/fixture/b".into(),
        Options {
            restore_name: Some("not-here".into()),
            ..Options::default()
        },
        1000,
    );
    header(&mut pane, "/fixture/b", 1000, 1040);
    assert!(!answer(&mut pane, 0, 320, 2, 1040));
    assert_eq!(pane.path(), "/fixture/a");
    assert!(pane.tick(1066));
    assert!(!pane.tick(11039));
    assert!(pane.tick(11040));
    assert_eq!((pane.path(), pane.held.start), ("/fixture/b", 0));
    assert!(!pane.busy());
}

#[test]
fn stale_refusal_keeps_a_park_but_a_located_window_or_failure_releases_it() {
    for located in [false, true] {
        let mut pane = settled();
        pane.begin(
            "/fixture/b".into(),
            Options {
                restore_name: Some("not-here".into()),
                ..Options::default()
            },
            1000,
        );
        header(&mut pane, "/fixture/b", 1000, 1040);
        assert!(!answer(&mut pane, 0, 320, 2, 1040));
        assert!(!pane.fail("stale", "paths", "stale", 0, 1041));
        assert!(pane.swap.pending.as_ref().unwrap().parked_until.is_some());
        if located {
            assert!(pane.anchor_window(680).is_some());
            assert!(!answer(&mut pane, 680, 320, 2, 1042));
            assert!(pane.tick(1042));
            assert_eq!(pane.held.start, 680);
        } else {
            assert!(pane.fail("window", "/fixture/b", "refused", 0, 1042));
            assert_eq!(pane.held.start, 0);
        }
        assert_eq!(pane.path(), "/fixture/b");
        assert!(!pane.busy());
    }
}
