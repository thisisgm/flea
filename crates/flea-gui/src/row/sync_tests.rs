use super::*;

#[test]
fn every_state_but_excluded_cuts_its_own_glyph_from_the_disc() {
    let keys: Vec<_> = SyncMark::ALL
        .into_iter()
        .filter_map(glyph)
        .map(|(key, _)| key)
        .collect();
    assert_eq!(keys.len(), SyncMark::ALL.len() - 1);
    for (at, key) in keys.iter().enumerate() {
        assert!(!keys[at + 1..].contains(key), "{key} is drawn once");
    }
    assert!(
        glyph(SyncMark::Excluded).is_none(),
        "excluded draws a hollow ring"
    );
}

#[test]
fn a_problem_reads_in_the_error_ink_and_a_finished_sync_in_the_tag_green() {
    assert_eq!(flea::ui_logic_rail_places::tagtheme::ROLES[GREEN], "green");
    let theme = Theme::default();
    assert_eq!(ink(SyncMark::Conflict, &theme), theme.error);
    assert_eq!(ink(SyncMark::Synced, &theme), theme.tag_listing_inks[GREEN]);
    assert_eq!(ink(SyncMark::Syncing, &theme), theme.accent);
}

#[test]
fn the_badge_sits_inside_the_marks_lower_right_corner_at_half_its_side() {
    let mark = Bounds::new(point(px(10.), px(20.)), size(px(24.), px(24.)));
    let badge = bounds(mark);
    assert_eq!(badge.size, size(px(12.), px(12.)));
    assert_eq!(badge.right(), mark.right());
    assert_eq!(badge.bottom(), mark.bottom());
    // A columns-sized mark keeps a readable badge that overhangs its corner by half of what it outgrew.
    let small = Bounds::new(point(px(0.), px(0.)), size(px(15.), px(15.)));
    let badge = bounds(small);
    assert_eq!(badge.size.width, px(MIN_SIDE));
    assert_eq!(badge.right(), small.right() + px((MIN_SIDE - 7.5) / 2.));
    assert_eq!(badge.bottom(), small.bottom() + px((MIN_SIDE - 7.5) / 2.));
    // Never larger than the mark itself.
    let tiny = Bounds::new(point(px(0.), px(0.)), size(px(6.), px(6.)));
    assert_eq!(bounds(tiny).size.width, px(6.));
}
