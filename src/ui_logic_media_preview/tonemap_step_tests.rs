use super::tonemap::{start_level, stepped, Source, Tonemap, FLOOR_SIDE};

const UHD: (u32, u32) = (3840, 2160);

fn hdr_4k() -> Tonemap {
    let mut tonemap = Tonemap::default();
    tonemap.source = Source {
        hdr: true,
        width: 3840,
        height: 2160,
        rotated: false,
        peak_nits: 0,
    };
    tonemap
}

#[test]
fn pv3_the_mapped_size_steps_by_three_quarters_down_to_the_floor() {
    let box_size = (1974, 1110);
    assert_eq!(stepped(box_size, 0), (1974, 1110));
    assert_eq!(stepped(box_size, 1), (1480, 832));
    assert_eq!(stepped(box_size, 2), (1110, 624));
    assert_eq!(stepped(box_size, 3), (832, 468));
    assert_eq!(stepped(box_size, 4), (640, FLOOR_SIDE));
    assert_eq!(stepped(box_size, 5), (640, FLOOR_SIDE));
    // A decode size at or under the floor never steps.
    assert_eq!(stepped((640, 360), 3), (640, 360));
    assert_eq!(stepped((360, 640), 2), (360, 640));
}

#[test]
fn pv3_the_first_size_keeps_the_load_within_the_start_budget() {
    // Quick Look: 4K30 one step down, 4K60 at the floor.
    assert_eq!(start_level(UHD, (1974, 1110), 29_970), 1);
    assert_eq!(start_level(UHD, (1974, 1110), 59_940), 4);
    // The pane: 4K30 at full size, 4K60 at the floor.
    assert_eq!(start_level(UHD, (752, 424), 29_970), 0);
    assert_eq!(start_level(UHD, (752, 424), 59_940), 1);
    assert_eq!(stepped((752, 424), 1), (638, FLOOR_SIDE));
    assert_eq!(start_level((1920, 1080), (1920, 1080), 29_970), 0);
    assert_eq!(start_level((1920, 1080), (1920, 1080), 59_940), 2);
    assert_eq!(start_level((640, 360), (640, 360), 240_000), 0);
}

#[test]
fn pv3_the_filter_starts_capped_steps_down_and_stops_at_the_floor() {
    let mut tonemap = hdr_4k();
    tonemap.set_rate(29.97);
    let first = tonemap.update(1974, 1110).unwrap();
    assert!(first.contains("zscale=w=1480:h=832:"), "{first}");
    assert_eq!(tonemap.size(), Some((1480, 832)));
    assert!(tonemap.step(1974, 1110));
    assert!(tonemap
        .update(1974, 1110)
        .unwrap()
        .contains("zscale=w=1110:h=624:"));
    assert!(tonemap.step(1974, 1110));
    assert!(tonemap.step(1974, 1110));
    assert!(tonemap
        .update(1974, 1110)
        .unwrap()
        .contains("zscale=w=640:h=360:"));
    assert!(!tonemap.step(1974, 1110));
    assert_eq!(tonemap.update(1974, 1110), None);
    // A smaller box keeps the level, so its size stays at the floor.
    assert!(tonemap
        .update(752, 424)
        .unwrap()
        .contains("zscale=w=638:h=360:"));
    assert_eq!(tonemap.level(), 4);
    // Given up, the filter has no size.
    assert!(tonemap.fail().is_some());
    assert_eq!(tonemap.size(), None);
}

#[test]
fn pv3_a_clip_that_is_not_hdr_never_enters_the_steps() {
    let mut tonemap = hdr_4k();
    tonemap.source.hdr = false;
    tonemap.set_rate(59.94);
    assert_eq!(tonemap.update(1974, 1110), None);
    assert!(!tonemap.step(1974, 1110));
    assert_eq!(tonemap.level(), 0);
    assert!(!tonemap.on());
}

#[test]
fn pv3_a_larger_box_gets_the_start_cap_again() {
    let mut tonemap = hdr_4k();
    tonemap.set_rate(59.94);
    tonemap.update(752, 424);
    assert_eq!(tonemap.size(), Some((638, FLOOR_SIDE)));
    // Quick Look's box: 4K60 starts at the floor there too, not at the pane's level.
    assert!(tonemap
        .update(1974, 1110)
        .unwrap()
        .contains("zscale=w=640:h=360:"));
    assert_eq!(tonemap.level(), 4);
}
