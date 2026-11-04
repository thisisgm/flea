//! The strip's one gesture: a parent opens on its first tap, and only inert space answers a double click.
use super::HOME;
use flea_gui::path::strip::{Hit, Strip, Tap};

fn fitted(path: &str, budget: usize) -> Strip {
    let mut strip = Strip::default();
    assert!(strip.fit(path, HOME, None, budget));
    strip
}

#[test]
fn a_parent_opens_on_the_first_tap_with_no_double_click_wait() {
    let mut strip = fitted("/home/gm/Work/claude", 80);
    assert_eq!(strip.press(Hit::Crumb(0), 1, 0), Tap::Open(HOME.into()));
    assert_eq!(
        strip.press(Hit::Crumb(1), 1, 0),
        Tap::Open("/home/gm/Work".into())
    );
    // A held modifier is another binding, and the chrome has none for it.
    assert_eq!(
        strip.press(Hit::Crumb(1), 1, flea::keys::CONTROL),
        Tap::Nothing
    );
}

#[test]
fn a_rapid_double_tap_on_a_parent_navigates_once_while_the_backend_is_slow() {
    let mut strip = fitted("/home/gm/Work/claude", 80);
    assert_eq!(
        strip.press(Hit::Crumb(1), 1, 0),
        Tap::Open("/home/gm/Work".into())
    );
    // The listing has not landed, so the strip still draws the same pieces when the second tap arrives.
    assert!(
        !strip.fit("/home/gm/Work/claude", HOME, None, 80),
        "an unchanged strip is not rebuilt"
    );
    assert_eq!(strip.press(Hit::Crumb(1), 2, 0), Tap::Nothing);
    assert_eq!(strip.press(Hit::Crumb(1), 3, 0), Tap::Nothing);
}

#[test]
fn a_second_tap_on_a_strip_that_moved_on_is_a_first_tap_on_what_it_meets() {
    let mut strip = fitted("/home/gm/Work/claude", 80);
    assert_eq!(
        strip.press(Hit::Crumb(1), 1, 0),
        Tap::Open("/home/gm/Work".into())
    );
    assert!(strip.fit("/home/gm/Work", HOME, None, 80));
    // The crumb under the pointer is now the leaf: a double click there edits instead of pairing with the parent.
    assert_eq!(strip.press(Hit::Crumb(1), 2, 0), Tap::Nothing);
    assert_eq!(strip.press(Hit::Crumb(1), 2, 0), Tap::Edit);
    // A first tap on another crumb never pairs with the tap before it.
    assert_eq!(strip.press(Hit::Crumb(0), 2, 0), Tap::Open(HOME.into()));
}

#[test]
fn the_leaf_the_marker_and_the_ground_are_inert_and_a_double_tap_there_edits() {
    let mut strip = fitted("/home/gm/one/two/three/four/five", 16);
    assert_eq!(strip.crumbs().len(), 4);
    assert!(strip.crumbs()[1].elided && strip.crumbs()[3].last);
    for hit in [Hit::Crumb(1), Hit::Crumb(3), Hit::Ground] {
        assert_eq!(strip.press(hit, 1, 0), Tap::Nothing, "{hit:?}");
        assert_eq!(strip.press(hit, 2, 0), Tap::Edit, "{hit:?}");
    }
    assert_eq!(
        strip.press(Hit::Crumb(2), 1, 0),
        Tap::Open("/home/gm/one/two/three/four".into())
    );
}

#[test]
fn a_hit_reads_the_shaped_widths_and_hover_lights_only_what_names_a_place() {
    let mut strip = fitted("/home/gm/one/two/three/four/five", 16);
    assert_eq!(
        strip.hit(20.),
        Hit::Ground,
        "an unplaced strip is all ground"
    );
    assert_eq!(strip.extent(0), None);
    // "~/" "\u{2026}/" "four/" "five" shaped at seven pixels a character.
    strip.place([14., 14., 35., 28.]);
    assert_eq!(strip.extent(2), Some((28., 35.)));
    assert_eq!(strip.hit(-1.), Hit::Ground);
    assert_eq!(strip.hit(0.), Hit::Crumb(0));
    assert_eq!(strip.hit(13.9), Hit::Crumb(0));
    assert_eq!(strip.hit(14.), Hit::Crumb(1));
    assert_eq!(strip.hit(28.), Hit::Crumb(2));
    assert_eq!(strip.hit(7. * 13.), Hit::Ground);
    assert!(strip.hover(Some(Hit::Crumb(2))));
    assert_eq!(strip.hovered, Some(2));
    assert!(!strip.hover(Some(Hit::Crumb(2))));
    assert!(
        strip.hover(Some(Hit::Crumb(1))),
        "the marker lights nothing"
    );
    assert_eq!(strip.hovered, None);
    strip.hover(Some(Hit::Crumb(3)));
    assert_eq!(strip.hovered, Some(3), "the leaf lights as it always draws");
    assert!(strip.hover(None));
}

#[test]
fn a_strip_builds_only_when_its_path_home_or_room_changes_and_a_cleared_one_builds_nothing() {
    let mut strip = Strip::default();
    assert_eq!(strip.built, 0);
    strip.clear();
    assert_eq!(
        (strip.built, strip.revision()),
        (0, 0),
        "clearing a strip that never built is free"
    );
    assert!(strip.fit("/usr/share", HOME, None, 40));
    for _ in 0..50 {
        assert!(!strip.fit("/usr/share", HOME, None, 40));
    }
    assert_eq!(strip.built, 1);
    assert!(strip.fit("/usr/share", HOME, None, 4));
    assert!(strip.fit("/usr/share", "", None, 4));
    assert_eq!(strip.built, 3);
    strip.clear();
    assert!(strip.crumbs().is_empty() && strip.extent(0).is_none() && strip.hit(0.) == Hit::Ground);
    assert_eq!(strip.built, 3);
}

#[test]
fn a_crumb_wider_than_its_characters_keeps_its_own_hit_and_pushes_the_next_one_along() {
    let mut strip = fitted("/home/gm/one/two/three/four/five", 16);
    // The marker "\u{2026}/" is two characters but draws 40 px, as a fallback face's glyph can.
    strip.place([14., 40., 35., 28.]);
    assert_eq!(strip.extent(1), Some((14., 40.)));
    assert_eq!(
        strip.extent(2),
        Some((54., 35.)),
        "the next crumb starts where the wide one ends"
    );
    assert_eq!(
        strip.hit(50.),
        Hit::Crumb(1),
        "a press on the wide glyph is that crumb"
    );
    assert_eq!(strip.hit(54.), Hit::Crumb(2));
    assert_eq!(strip.hit(116.9), Hit::Crumb(3));
    assert_eq!(strip.hit(117.), Hit::Ground);
    // A short width list places only what it names; the rest is ground until the next place.
    strip.place([14.]);
    assert_eq!(
        (strip.hit(10.), strip.hit(20.)),
        (Hit::Crumb(0), Hit::Ground)
    );
}
