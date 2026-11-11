use super::strip::*;
use super::Sound;

const METRICS: Metrics = Metrics {
    gap: 9.,
    slot: 24.,
    body_small: 13.,
};

// tests/mediastrip-volume.qml stepLayout at a 640 wide strip, with the clock's own width.
#[test]
fn pv3_the_unframed_strip_lays_out_as_the_board_draws_it() {
    let clock = 77.;
    let strip = layout(640., false, clock, METRICS);
    let volume = strip
        .volume
        .expect("the unframed strip holds its volume track");
    assert_eq!(640. - volume.right(), 9., "one gap from the right edge");
    assert_eq!(volume.w, 60.);
    assert_eq!(strip.mute.w, 24., "the volume slot is 24 wide");
    assert_eq!(
        volume.x - strip.mute.right(),
        9.,
        "the mark sits one gap left of the track"
    );
    assert_eq!(
        strip.mute.x - strip.clock.right(),
        9.,
        "the clock is one gap left of the mark"
    );
    assert_eq!(
        strip.clock.x - strip.seek.right(),
        9.,
        "the seek fills to one gap before the clock"
    );
    assert_eq!(
        [strip.play.x, strip.play.w, strip.seek.x],
        [9., 24., 9. + 24. + 9.]
    );
}

#[test]
fn pv3_the_framed_strip_keeps_the_mark_on_the_right_edge() {
    let strip = layout(640., true, 77., METRICS);
    assert_eq!(strip.volume, None, "framed: no volume track exists");
    assert_eq!([640. - strip.mute.right(), strip.mute.w], [9., 24.]);
    assert_eq!(strip.mute.x - strip.clock.right(), 9.);
    assert_eq!(strip.clock.x - strip.seek.right(), 9.);
    assert_eq!([strip.play.x, strip.seek.x], [9., 42.]);
}

#[test]
fn pv3_a_strip_too_narrow_for_its_parts_gives_the_seek_no_width() {
    let strip = layout(120., false, 77., METRICS);
    assert_eq!(strip.seek.w, 0.);
}

// stepSize14 and stepSize12: bodySmall 13 draws 60 px, bodySmall 11 draws 51.
#[test]
fn pv3_the_volume_track_scales_with_the_text_size() {
    assert_eq!(volume_width(13.), 60.);
    assert_eq!(volume_width(11.), 51.);
    assert_eq!(handle(12.), 6.);
    assert_eq!(handle(14.), 7.);
}

#[test]
fn pv3_fractions_clamp_and_ignore_a_missing_duration() {
    assert!((fraction(47_000, 192_000) - 47. / 192.).abs() < 1e-6);
    assert_eq!(fraction(5, 0), 0.);
    assert_eq!(fraction(-5, 10), 0.);
    assert_eq!(fraction(50, 10), 1.);
    assert_eq!(fraction_at(30., 60.), 0.5);
    assert_eq!(fraction_at(-3., 60.), 0.);
    assert_eq!(fraction_at(90., 60.), 1.);
    assert_eq!(fraction_at(0.5, 0.), 0.5, "a zero width divides by one");
    assert_eq!(level_at(45., 60.), 75);
    assert_eq!(handle_x(60., 1., 6.), 57.);
    assert_eq!(handle_x(60., 0., 6.), -3.);
}

// The seek track seeks once, on release, and draws the pointer while dragging.
#[test]
fn pv3_a_scrub_seeks_once_on_release() {
    let mut scrub = Scrub::default();
    scrub.moved(30., 60.);
    assert!(!scrub.active, "a hover is no drag");
    scrub.press(15., 60.);
    assert_eq!(scrub.shown(0.9), 0.25);
    scrub.moved(30., 60.);
    assert_eq!(scrub.shown(0.9), 0.5);
    assert_eq!(scrub.release(192_000), Some(96_000));
    assert_eq!(scrub.shown(0.9), 0.9);
    scrub.press(30., 60.);
    assert_eq!(scrub.release(0), None, "no duration, no seek");
    assert!(!scrub.active);
}

#[test]
fn pv3_a_wheel_click_steps_one_second() {
    assert_eq!(wheel_seek(47_000, 192_000, true), Some(48_000));
    assert_eq!(wheel_seek(47_000, 192_000, false), Some(46_000));
    assert_eq!(wheel_seek(47_000, 0, true), None);
}

fn x_for(level: u8, width: f32) -> f32 {
    (0..=width as u32)
        .map(|x| x as f32)
        .find(|x| level_at(*x, width) == level)
        .unwrap()
}

// stepDrag, stepHover, stepClick, stepCancel and stepDestroy on the volume track.
#[test]
fn pv3_the_volume_press_is_heard_live_and_kept_on_release() {
    let mut sound = Sound::new(60);
    let mut level = Level::default();
    level.moved(&mut sound, x_for(30, 60.), 60., false);
    assert_eq!(sound.volume(), 60, "a hover leaves the level alone");
    level.press(&mut sound, x_for(90, 60.), 60.);
    assert_eq!(sound.volume(), 90);
    level.moved(&mut sound, x_for(75, 60.), 60., true);
    assert_eq!((sound.volume(), sound.stored), (75, 60));
    assert_eq!(level.release(&mut sound), Some(75));
    assert_eq!((sound.held, level.pressing), (None, false));
    level.press(&mut sound, x_for(20, 60.), 60.);
    assert_eq!(
        level.release(&mut sound),
        Some(20),
        "a click sets and keeps the level"
    );
    level.press(&mut sound, x_for(20, 60.), 60.);
    assert_eq!(
        level.release(&mut sound),
        None,
        "the same level again stores nothing"
    );
    level.press(&mut sound, x_for(40, 60.), 60.);
    level.moved(&mut sound, x_for(35, 60.), 60., true);
    assert_eq!(
        level.release(&mut sound),
        Some(35),
        "a stolen grab keeps the level it named"
    );
    level.press(&mut sound, x_for(10, 60.), 60.);
    level.moved(&mut sound, x_for(5, 60.), 60., true);
    assert_eq!(sound.volume(), 5);
    level.destroyed(&mut sound);
    assert_eq!((sound.volume(), sound.stored, sound.held), (35, 35, None));
    level.destroyed(&mut sound);
    assert_eq!(sound.volume(), 35, "no press, nothing to drop");
}
