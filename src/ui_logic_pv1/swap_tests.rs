// tests/js/previewswap.js value checks: the preview swap's moves, cap, frame kinds, readiness and the settle.
use super::kinds::State;
use super::settle::{self, Plan};
use super::swap::*;

fn column(state: State, edit: impl FnOnce(&mut ColumnProbe)) -> bool {
    let mut p = ColumnProbe::new(state);
    edit(&mut p);
    column_ready(&p)
}

#[test]
fn the_cap_and_the_capture_window() {
    // a preview gets the listing swap's cap once its own work starts, and a PDF the same
    assert_eq!((cap_ms(false), cap_ms(true)), (CAP_MS, CAP_MS));
}

// a capture gives up before the preview column's own 120 ms settle would load under it
const _: () = assert!(CAPTURE_MS < settle::SETTLE_MS);

#[test]
fn moves_hold_join_or_burst() {
    let idle = MoveState::default();
    let held = MoveState { capturing: false, holding: true, key: Some("/d\n3".into()) };
    assert_eq!(on_move(&idle, "/d\n3", true), Move::Hold);
    // the second caller for the same row joins it
    assert_eq!(on_move(&held, "/d\n3", true), Move::Join);
    // a move to another row during a column hold is held j, which gives the stale picture up
    assert_eq!(on_move(&held, "/d\n4", true), Move::Burst);
    // Quick Look keeps its picture through held j
    assert_eq!(on_move(&held, "/d/b.jpg", false), Move::Join);
    // a capture still in flight counts as a hold
    let capturing = MoveState { capturing: true, holding: false, key: Some("/d\n3".into()) };
    assert_eq!(on_move(&capturing, "/d\n4", true), Move::Burst);
}

#[test]
fn frame_kinds() {
    assert_eq!(frame_kind(true, false, false, false), Frame::Held);
    assert_eq!(frame_kind(false, true, false, false), Frame::Whole);
    // a half-built preview drawn live is the defect
    assert_eq!(frame_kind(false, false, false, false), Frame::Mid);
    assert_eq!(frame_kind(false, false, true, false), Frame::Loading);
    assert_eq!(frame_kind(true, true, false, true), Frame::Loading);
}

#[test]
fn what_each_column_kind_waits_for() {
    assert!(!column(State::Loading, |_| {}));
    assert!(!column(State::Image, |p| (p.thumb, p.frame) = (true, ImageStatus::Loading)));
    assert!(column(State::Image, |p| (p.thumb, p.frame) = (true, ImageStatus::Ready)));
    assert!(column(State::Image, |p| (p.thumb, p.frame) = (true, ImageStatus::Error)));
    // an image with no cache file yet but one coming waits for it
    assert!(!column(State::Image, |_| {}));
    assert!(!column(State::Image, |p| (p.no_thumb_coming, p.frame) = (true, ImageStatus::Loading)));
    assert!(column(State::Image, |p| (p.no_thumb_coming, p.frame) = (true, ImageStatus::Ready)));
    assert!(column(State::Video, |p| p.no_thumb_coming = true));
    assert!(!column(State::Video, |p| (p.thumb, p.frame) = (true, ImageStatus::Loading)));
    assert!(!column(State::Pdf, |_| {}) && column(State::Pdf, |p| p.pdf_drawn = true) && column(State::Pdf, |p| p.pdf_failed = true));
    assert!(!column(State::Text, |p| p.lines_loading = true) && column(State::Code, |_| {}));
    assert!(!column(State::Archive, |_| {}) && column(State::Archive, |p| p.meta = true));
    // a kind drawn only as its mark lands with its facts
    for state in [State::Unsupported, State::Audio, State::Symlink, State::Multi] {
        assert!(column(state, |_| {}), "{state:?}");
    }
}

#[test]
fn what_quick_look_waits_for() {
    assert!(!look_ready(true, false, false, false, false, false));
    // an interim shown is whole, over a PDF too, and so is a Markdown head
    assert!(look_ready(true, false, false, false, true, false));
    assert!(look_ready(true, true, false, false, true, false));
    assert!(look_ready(true, false, false, false, false, true));
    assert!(!look_ready(true, true, false, false, false, false));
    // a PDF viewer is ready once a page is on screen or the document is refused
    assert!(!look_ready(false, true, false, false, false, false));
    assert!(look_ready(false, true, true, false, false, false));
    assert!(look_ready(false, true, false, true, false, false));
    assert!(look_ready(false, false, false, false, false, false));
}

fn rect(sw: f64, sh: f64, w: f64, h: f64, orient: u32) -> String {
    interim_rect(sw, sh, w, h, orient).map_or("none".into(), |r| format!("{},{},{}x{}", r.x, r.y, r.w, r.h))
}

#[test]
fn the_interim_rect_is_the_originals_upright_fit() {
    assert_eq!(rect(754.0, 471.0, 6000.0, 4000.0, 1), "24,0,706.5x471");
    assert_eq!(rect(754.0, 471.0, 6000.0, 4000.0, 3), "24,0,706.5x471");
    assert_eq!(rect(2080.0, 1137.0, 4032.0, 3024.0, 6), "614,0,852.75x1137");
    assert_eq!(rect(2080.0, 1137.0, 3024.0, 4032.0, 8), "282,0,1516x1137");
    assert_eq!(rect(100.0, 100.0, 60.0, 40.0, 6), "30,20,40x60");
    assert_eq!(rect(100.0, 100.0, 60.0, 40.0, 1), "20,30,60x40");
    // a small original is never enlarged
    assert_eq!(rect(754.0, 471.0, 120.0, 68.0, 1), "317,202,120x68");
    assert_eq!(rect(754.0, 471.0, 0.0, 0.0, 1) + &rect(754.0, 471.0, 640.0, 0.0, 1) + &rect(754.0, 471.0, 0.0, 480.0, 6), "nonenonenone");
}

#[test]
fn release_and_fallback_owe_the_loading_state_its_minimum() {
    assert_eq!(release_in(false, true, 0, 10), None);
    assert_eq!(release_in(true, false, 0, 10), None);
    assert_eq!(release_in(true, true, 0, 10), Some(0));
    assert_eq!(release_in(true, true, 1000, 1040), Some(60));
    assert_eq!(fall_back(true, 0, 1066), FallBack { shown_at: 1066, left: 100, keeps: true });
    assert_eq!(fall_back(false, 1000, 1200), FallBack { shown_at: 1000, left: 0, keeps: false });
    assert_eq!(show_left(0, 50), 0);
}

#[test]
fn the_settle() {
    assert_eq!(settle::selection_key(0, 9), "");
    assert_eq!(settle::selection_key(1, 7), "");
    assert_eq!(settle::selection_key(3, 4), "3:4");
    assert_eq!(settle::selection_key(3, 5), "3:5");
    assert!(settle::due(1000, 0, 120) && settle::due(120, 0, 120) && !settle::due(119, 0, 120));
    // a backwards clock reads idle, never wedged
    assert!(settle::due(100, 200, 120));
    assert_eq!(settle::plan(50, 0, 120, "b", "b", true), Plan::Same);
    assert_eq!(settle::plan(1000, 0, 120, "b", "b", false), Plan::Now);
    assert_eq!(settle::plan(1000, 0, 120, "c", "b", false), Plan::Now);
    assert_eq!(settle::plan(50, 0, 120, "c", "b", false), Plan::Later);
    // a 30ms burst loads first once, then trails only
    let (mut last_at, mut last_key, mut armed, mut seen) = (-1000, "a", false, Vec::new());
    for (at, key) in [(0, "b"), (30, "c"), (60, "d"), (90, "e"), (120, "f")] {
        let decision = settle::plan(at, last_at, 120, key, last_key, armed);
        seen.push(decision);
        if decision != Plan::Same {
            (last_key, last_at, armed) = (key, at, decision == Plan::Later);
        }
    }
    assert_eq!(seen, [Plan::Now, Plan::Later, Plan::Later, Plan::Later, Plan::Later]);
    assert_eq!(settle::plan(100, 90, 120, "e", "e", true), Plan::Same);
}

#[test]
fn a_pdf_opens_only_when_idle_and_no_render_runs() {
    assert!(settle::pdf_open_now(1000, 0, 120, false));
    assert!(settle::pdf_open_now(120, 0, 120, false));
    assert!(!settle::pdf_open_now(50, 0, 120, false));
    assert!(!settle::pdf_open_now(1000, 0, 120, true));
    assert!(!settle::pdf_open_now(50, 0, 120, true));
    assert!(settle::pdf_open_now(100, 200, 120, false));
}
