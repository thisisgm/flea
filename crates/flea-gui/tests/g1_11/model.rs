use super::rail::{
    geometry::{drawn_rows, place, width_stop, Rect, DEFAULT_WIDTH, EDGE, ROW},
    Counters, Handoff, Mode, Plate, Rail, CLOSE_MS, DWELL_MS, OPEN_MS, SETTLE_MS,
};
use super::{at, column, point};
use std::time::Instant;

fn places(text: &str) -> Mode {
    Mode::from_places(&flea::jsondoc::parse(text).unwrap())
}

// Samples the slide every 16 ms through a motion: ordered, no jump over 60%, at least three positions.
fn sweep(rail: &Rail, origin: Instant, from: u64, to: u64) -> Vec<f32> {
    (from..=to)
        .step_by(16)
        .map(|ms| rail.slide(at(origin, ms)))
        .collect()
}

fn smooth(samples: &[f32], rising: bool) {
    for pair in samples.windows(2) {
        assert!(
            if rising {
                pair[1] >= pair[0]
            } else {
                pair[1] <= pair[0]
            },
            "{samples:?}"
        );
        assert!((pair[1] - pair[0]).abs() <= 0.6, "{samples:?}");
    }
    let mut distinct = samples.to_vec();
    distinct.dedup();
    assert!(distinct.len() >= 3, "{samples:?}");
}

fn revealed_overlay(origin: Instant) -> Rail {
    let mut rail = Rail::new(Mode::Overlay, false, origin);
    rail.edge(true, origin);
    assert!(rail.tick(at(origin, DWELL_MS)));
    rail
}

#[test]
fn places_settings_choose_the_mode_and_auto_hide_wins() {
    assert_eq!(places("{}"), Mode::Docked);
    assert_eq!(places(r#"{"rail":"shown","autoHide":false}"#), Mode::Docked);
    assert_eq!(places(r#"{"rail":"hidden"}"#), Mode::Hidden);
    assert_eq!(
        places(r#"{"rail":"hidden","autoHide":true}"#),
        Mode::Overlay
    );
    assert_eq!(places(r#"{"autoHide":true}"#), Mode::Overlay);
}

#[test]
fn width_snaps_to_the_settings_stops() {
    assert_eq!(width_stop(None), DEFAULT_WIDTH);
    assert_eq!(width_stop(Some(f64::NAN)), 192.);
    assert_eq!(width_stop(Some(999.)), 256.);
    assert_eq!(width_stop(Some(0.)), 160.);
    assert_eq!(width_stop(Some(200.)), 192.);
    // A tie keeps the lower stop, as Places.sidebarWidth does.
    assert_eq!(width_stop(Some(208.)), 192.);
    assert_eq!(width_stop(Some(240.)), 224.);
    assert_eq!((ROW, EDGE), (29., 14.));
}

#[test]
fn docked_owes_its_width_and_hidden_and_overlay_owe_nothing() {
    let docked = place(Mode::Docked, 192., column(), 1.);
    assert_eq!(docked.inset, 192.);
    assert_eq!(docked.rail, Rect::new(0., 27., 192., 646.));
    assert_eq!((docked.strip.width, docked.drop.width), (0., 0.));
    let hidden = place(Mode::Hidden, 192., column(), 0.);
    assert_eq!((hidden.inset, hidden.rail.width), (0., 0.));
    let parked = place(Mode::Overlay, 192., column(), 0.);
    assert_eq!(parked.rail, Rect::new(-192., 27., 192., 646.));
    assert_eq!(parked.strip, Rect::new(0., 27., EDGE, 646.));
    assert_eq!(parked.drop, Rect::new(0., 27., 192., 646.));
    assert_eq!(place(Mode::Overlay, 192., column(), 1.).rail.x, 0.);
}

#[test]
fn an_overlay_reveal_never_shifts_the_panes_or_resizes_the_rail() {
    let origin = Instant::now();
    let rail = revealed_overlay(origin);
    let mut insets = Vec::new();
    let mut widths = Vec::new();
    for ms in (DWELL_MS..=DWELL_MS + OPEN_MS).step_by(8) {
        let placed = rail.placement(192., column(), at(origin, ms));
        insets.push(placed.inset);
        widths.push(placed.rail.width);
    }
    assert!(insets.iter().all(|inset| *inset == 0.), "{insets:?}");
    assert!(widths.iter().all(|width| *width == 192.), "{widths:?}");
}

#[test]
fn launch_builds_nothing_and_a_short_crossing_never_reveals() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Overlay, false, origin);
    assert!(!rail.live() && !rail.polling() && !rail.drawn(origin));
    assert_eq!(rail.counters, Counters::default());
    rail.edge(true, origin);
    assert_eq!(
        rail.deadline(origin),
        Some(std::time::Duration::from_millis(DWELL_MS))
    );
    rail.edge(false, at(origin, DWELL_MS - 20));
    assert_eq!(rail.deadline(at(origin, DWELL_MS - 20)), None);
    assert!(!rail.tick(at(origin, 1000)));
    // The entry built the rows, undrawn, parked at minus the width.
    assert!(rail.live() && !rail.shown() && !rail.drawn(at(origin, 1000)));
    assert_eq!(
        rail.placement(192., column(), at(origin, 1000)).rail.x,
        -192.
    );
    assert_eq!((rail.counters.builds, rail.counters.reveals), (1, 0));
}

#[test]
fn the_dwell_reveals_and_leaving_waits_the_settle() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Overlay, false, origin);
    rail.edge(true, origin);
    assert!(!rail.tick(at(origin, DWELL_MS - 1)));
    assert!(!rail.shown());
    assert!(rail.tick(at(origin, DWELL_MS)));
    assert!(rail.shown() && rail.polling());
    assert_eq!(rail.plate(), Plate::Grey);
    smooth(&sweep(&rail, origin, DWELL_MS, DWELL_MS + OPEN_MS), true);
    assert_eq!(rail.slide(at(origin, DWELL_MS + OPEN_MS)), 1.);
    assert!(!rail.animating(at(origin, DWELL_MS + OPEN_MS)));
    let out = 1000;
    rail.over(true, at(origin, 400));
    rail.edge(false, at(origin, 500));
    rail.over(false, at(origin, out));
    assert!(!rail.tick(at(origin, out + SETTLE_MS - 1)));
    assert!(rail.shown());
    assert!(rail.tick(at(origin, out + SETTLE_MS)));
    let hide = out + SETTLE_MS;
    assert!(!rail.shown() && !rail.polling());
    smooth(&sweep(&rail, origin, hide, hide + CLOSE_MS), false);
    assert!(rail.drawn(at(origin, hide + CLOSE_MS - 1)));
    assert!(!rail.drawn(at(origin, hide + CLOSE_MS)));
    assert!(rail.live());
}

#[test]
fn reentry_mid_hide_reverses_from_where_the_rail_stands() {
    let origin = Instant::now();
    let mut rail = revealed_overlay(origin);
    rail.edge(false, at(origin, 400));
    assert!(rail.tick(at(origin, 400 + SETTLE_MS)));
    let mid = 400 + SETTLE_MS + CLOSE_MS / 2;
    let there = rail.slide(at(origin, mid));
    assert!(there > 0. && there < 1., "{there}");
    rail.over(true, at(origin, mid));
    assert!(rail.shown());
    assert_eq!(rail.deadline(at(origin, mid)), None);
    let back = sweep(&rail, origin, mid, mid + OPEN_MS);
    assert_eq!(back[0], there);
    smooth(&back, true);
}

#[test]
fn three_reveals_keep_one_build() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Overlay, false, origin);
    let mut t = 0;
    for _ in 0..3 {
        rail.edge(true, at(origin, t));
        rail.tick(at(origin, t + DWELL_MS));
        assert!(rail.shown());
        rail.edge(false, at(origin, t + 300));
        rail.tick(at(origin, t + 300 + SETTLE_MS));
        assert!(!rail.shown());
        t += 2000;
    }
    assert_eq!((rail.counters.builds, rail.counters.reveals), (1, 3));
}

#[test]
fn tab_in_reveals_at_once_and_tab_out_skips_the_settle() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Overlay, false, origin);
    rail.focus(true, origin);
    assert!(rail.shown() && rail.live());
    assert_eq!(rail.plate(), Plate::Accent);
    assert_eq!(rail.deadline(origin), None);
    rail.focus(false, at(origin, 300));
    assert!(!rail.shown());
    assert_eq!(rail.deadline(at(origin, 300)), None);
    assert_eq!(rail.plate(), Plate::Grey);
    // The pointer on the rail keeps it up through a Tab out.
    rail.focus(true, at(origin, 1000));
    rail.over(true, at(origin, 1001));
    rail.focus(false, at(origin, 1002));
    assert!(rail.shown());
}

#[test]
fn a_menu_a_drag_and_a_release_each_hold_the_overlay_up() {
    let origin = Instant::now();
    type Hold = fn(&mut Rail, bool, Instant);
    let holds: [(&str, Hold); 3] = [
        ("menu", Rail::menu),
        ("drag", Rail::drag),
        ("release", Rail::releasing),
    ];
    for (name, hold) in holds {
        let mut rail = revealed_overlay(origin);
        hold(&mut rail, true, at(origin, 300));
        rail.edge(false, at(origin, 310));
        assert_eq!(rail.deadline(at(origin, 310)), None, "{name}");
        assert!(!rail.tick(at(origin, 5000)) && rail.shown(), "{name}");
        hold(&mut rail, false, at(origin, 6000));
        assert!(rail.tick(at(origin, 6000 + SETTLE_MS)), "{name}");
        assert!(!rail.shown(), "{name}");
    }
}

#[test]
fn a_release_keeps_the_rows_alive_undrawn_through_a_hide() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Docked, false, origin);
    assert_eq!(rail.counters.builds, 1);
    rail.releasing(true, origin);
    assert_eq!(rail.set_mode(Mode::Hidden, origin), None);
    assert!(rail.live() && !rail.drawn(origin) && !rail.polling());
    rail.releasing(false, at(origin, 10));
    assert!(!rail.live());
    assert_eq!(rail.counters.builds, 1);
}

#[test]
fn a_hidden_rail_starts_no_work_and_hiding_returns_the_keyboard() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Hidden, false, origin);
    rail.edge(true, origin);
    rail.over(true, origin);
    rail.menu(true, origin);
    rail.menu(false, origin);
    assert!(!rail.tick(at(origin, 10_000)));
    assert!(!rail.live() && !rail.polling() && !rail.drawn(origin));
    assert_eq!(rail.counters, Counters::default());
    let mut docked = Rail::new(Mode::Docked, false, origin);
    docked.focus(true, origin);
    assert_eq!(docked.plate(), Plate::Accent);
    assert_eq!(docked.set_mode(Mode::Hidden, origin), Some(Handoff::List));
    assert_eq!(docked.plate(), Plate::Grey);
    // Docked to overlay keeps nothing revealed until the pointer asks.
    let mut shown = Rail::new(Mode::Docked, false, origin);
    shown.focus(true, origin);
    assert_eq!(shown.set_mode(Mode::Overlay, origin), Some(Handoff::List));
    assert!(!shown.live() && !shown.shown());
}

#[test]
fn reduced_motion_jumps_but_keeps_the_dwell_and_settle() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Overlay, true, origin);
    rail.edge(true, origin);
    assert!(!rail.shown());
    rail.tick(at(origin, DWELL_MS));
    assert_eq!(rail.slide(at(origin, DWELL_MS)), 1.);
    assert!(!rail.animating(at(origin, DWELL_MS)));
    rail.edge(false, at(origin, 200));
    assert!(rail.tick(at(origin, 200 + SETTLE_MS)));
    assert_eq!(rail.slide(at(origin, 200 + SETTLE_MS)), 0.);
    assert!(!rail.drawn(at(origin, 200 + SETTLE_MS)));
}

#[test]
fn presses_hand_the_keyboard_to_the_owner_of_the_point() {
    let origin = Instant::now();
    let docked = Rail::new(Mode::Docked, false, origin);
    let placed = docked.placement(192., column(), origin);
    assert_eq!(docked.press(&placed, point(50., 100.), origin), None);
    assert_eq!(
        docked.press(&placed, point(300., 100.), origin),
        Some(Handoff::List)
    );
    assert_eq!(docked.press(&placed, point(300., 10.), origin), None);
    assert!(!docked.refuses_drop(&placed, point(50., 100.)));
    let mut overlay = Rail::new(Mode::Overlay, false, origin);
    let parked = overlay.placement(192., column(), origin);
    // The strip holds no press handler: a click there lands on the listing.
    assert_eq!(
        overlay.press(&parked, point(5., 100.), origin),
        Some(Handoff::List)
    );
    assert!(overlay.refuses_drop(&parked, point(5., 100.)));
    assert!(!overlay.refuses_drop(&parked, point(100., 100.)));
    overlay.focus(true, origin);
    let up = at(origin, OPEN_MS);
    let shown = overlay.placement(192., column(), up);
    assert_eq!(
        overlay.press(&shown, point(50., 100.), up),
        Some(Handoff::Rail)
    );
    assert_eq!(
        overlay.press(&shown, point(300., 100.), up),
        Some(Handoff::List)
    );
}

#[test]
fn drawn_rows_are_bounded_by_the_rail_height() {
    assert_eq!(drawn_rows(0.), 0);
    assert_eq!(drawn_rows(646.), 24);
    assert_eq!(drawn_rows(29.), 2);
}

// ui/PaneRail.qml dragReveal is max(strip, rail) and the rail is 0 wide until built and while a release holds it.
#[test]
fn a_drop_lane_is_the_strip_until_the_rows_have_a_width() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Overlay, false, origin);
    let parked = rail.placement(192., column(), origin);
    assert!(!rail.live());
    assert!(rail.refuses_drop(&parked, point(5., 300.)));
    assert!(!rail.refuses_drop(&parked, point(100., 300.)));
    rail.edge(true, origin);
    assert!(rail.live() && !rail.shown());
    assert!(rail.refuses_drop(&parked, point(100., 300.)));
    assert!(!rail.refuses_drop(&parked, point(192., 300.)));
    // A release hold carried into auto-hide keeps the rows alive but 0 wide.
    let mut held = Rail::new(Mode::Docked, false, origin);
    held.releasing(true, origin);
    held.set_mode(Mode::Overlay, origin);
    assert!(held.live() && !held.drawn(origin));
    let placed = held.placement(192., column(), origin);
    assert!(held.refuses_drop(&placed, point(5., 300.)));
    assert!(!held.refuses_drop(&placed, point(100., 300.)));
}

// A docked rail is drawn whatever its menu does; the hold applies to an overlay only.
#[test]
fn a_docked_rail_stays_drawn_under_its_menu() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Docked, false, origin);
    rail.menu(true, origin);
    let placed = rail.placement(192., column(), at(origin, 5000));
    assert!(rail.shown() && rail.drawn(at(origin, 5000)));
    assert_eq!(rail.deadline(origin), None);
    assert_eq!(placed.rail, Rect::new(0., 27., 192., 646.));
    rail.menu(false, at(origin, 6000));
    assert!(rail.shown() && rail.drawn(at(origin, 9000)) && rail.deadline(origin).is_none());
}

// boards/list.html and the docked rail-menu boards: the active place is grey until the rail holds the keyboard.
#[test]
fn a_docked_rail_plates_grey_until_it_holds_the_keyboard() {
    let origin = Instant::now();
    let mut rail = Rail::new(Mode::Docked, false, origin);
    assert_eq!(rail.plate(), Plate::Grey);
    rail.focus(true, origin);
    assert_eq!(rail.plate(), Plate::Accent);
    rail.focus(false, at(origin, 10));
    assert_eq!(rail.plate(), Plate::Grey);
    assert!(rail.shown() && rail.drawn(at(origin, 10)));
}

// ci/visual/g1/11.json g1/rail-reveal: the pointer rests on the strip until the slide lands, so the capture sees x 0.
#[test]
fn the_rail_reveal_scene_rests_until_the_slide_lands() {
    let manifest = flea::jsondoc::parse(include_str!("../../../../ci/visual/g1/11.json")).unwrap();
    let scenes = manifest.get("scenes").and_then(|s| s.as_array()).unwrap();
    let reveal = scenes
        .iter()
        .find(|s| s.get("scene").and_then(|v| v.as_str()) == Some("g1/rail-reveal"))
        .unwrap();
    let input = reveal
        .get("setup")
        .and_then(|s| s.get("input"))
        .and_then(|i| i.as_array())
        .unwrap();
    let rest = input[0].get("dwell_ms").and_then(|v| v.as_f64()).unwrap() as u64;
    let origin = Instant::now();
    let rail = revealed_overlay(origin);
    let placed = rail.placement(192., column(), at(origin, rest));
    assert_eq!(
        (placed.rail.x, placed.rail.width, placed.inset),
        (0., 192., 0.),
        "rest {rest} ms"
    );
    assert!(rail.drawn(at(origin, rest)) && !rail.animating(at(origin, rest)));
}
