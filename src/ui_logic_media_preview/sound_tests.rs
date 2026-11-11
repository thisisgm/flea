use super::*;

fn near(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() < 0.000_001
}

// tests/mediastrip-volume.qml stepGain: Qt's convertVolume for a few levels.
#[test]
fn pv3_gain_maps_the_level_through_the_logarithmic_scale() {
    // -ln(1 - level / 100) / ln(100): half the scale left is log10(2) / 2, a quarter left is log10(2).
    let half = std::f64::consts::LOG10_2 / 2.;
    for (level, want) in [
        (0, 0.),
        (50, half),
        (75, std::f64::consts::LOG10_2),
        (100, 1.),
    ] {
        assert!(near(gain(level), want), "level {level}: {}", gain(level));
    }
    let mut sound = Sound::new(60);
    sound.set_volume(50);
    assert!(near(sound.gain(), half));
    sound.held = None;
    assert_eq!(
        sound.volume(),
        60,
        "no press down reads the stored level again"
    );
}

// stepDrag and stepStored: moves are heard live, only the release keeps one patch.
#[test]
fn pv3_a_drag_sets_the_level_live_and_keeps_it_once() {
    let mut sound = Sound::new(60);
    for level in [90, 80, 75] {
        sound.set_volume(level);
        assert_eq!(sound.volume(), level);
        assert_eq!(sound.stored, 60, "no move stored anything");
    }
    assert_eq!(sound.keep_volume(), Some(75));
    assert_eq!((sound.volume(), sound.stored, sound.held), (75, 75, None));
    assert_eq!(volume_patch(75), r#"{"preview":{"volume":75}}"#);
}

// stepClickStored and stepMute: the same level again stores nothing, and mute leaves the level alone.
#[test]
fn pv3_the_same_level_stores_nothing_and_mute_keeps_the_level() {
    let mut sound = Sound::new(20);
    sound.set_volume(20);
    assert_eq!(sound.keep_volume(), None);
    sound.toggle();
    assert!(sound.muted);
    assert_eq!((sound.volume(), sound.stored), (20, 20));
    sound.toggle();
    assert_eq!((sound.muted, sound.volume()), (false, 20));
}

// stepDestroy: a strip destroyed under a held press drops the dragged level.
#[test]
fn pv3_a_destroyed_press_drops_the_dragged_level() {
    let mut sound = Sound::new(35);
    sound.set_volume(5);
    assert_eq!(sound.volume(), 5);
    sound.drop_press();
    assert_eq!((sound.volume(), sound.stored, sound.held), (35, 35, None));
}

// stepAccess and stepAccessDown: a screen reader steps by 5 and keeps it, clamped to the scale.
#[test]
fn pv3_a_screen_reader_steps_by_five_and_keeps_it() {
    let mut sound = Sound::new(35);
    assert_eq!(sound.nudge(5), Some(40));
    assert_eq!(sound.nudge(-5), Some(35));
    assert_eq!(sound.nudge(-5), Some(30));
    assert_eq!(sound.held, None);
    let mut top = Sound::new(98);
    assert_eq!(top.nudge(5), Some(100));
    assert_eq!(top.nudge(5), None, "the top of the scale keeps nothing new");
    let mut bottom = Sound::new(2);
    assert_eq!(bottom.nudge(-5), Some(0));
}

#[test]
fn pv3_a_stored_level_past_the_scale_is_clamped() {
    assert_eq!(Sound::new(250).volume(), FULL_LEVEL);
    let mut sound = Sound::new(10);
    sound.set_volume(200);
    assert_eq!(sound.volume(), FULL_LEVEL);
}

#[test]
fn pv3_duration_reads_as_a_player_clock() {
    assert_eq!(duration(0), "0:00");
    assert_eq!(duration(-5), "0:00");
    assert_eq!(duration(2_999), "0:02");
    assert_eq!(duration(47_000), "0:47");
    assert_eq!(duration(192_000), "3:12");
    assert_eq!(duration(3_785_000), "1:03:05");
}

#[test]
fn pv3_sample_rate_reads_whole_kilohertz_without_a_decimal() {
    assert_eq!(sample_rate_khz(48_000).as_deref(), Some("48"));
    assert_eq!(sample_rate_khz(44_100).as_deref(), Some("44.1"));
    assert_eq!(sample_rate_khz(0), None);
    assert_eq!(sample_rate_khz(-1), None);
}

// ql-audio board: "Ogg audio · 0:02 · 44.1 kHz · 6.9 kB".
#[test]
fn pv3_facts_lead_with_the_kind_and_drop_empty_parts() {
    assert_eq!(
        facts("/m/tone.ogg", "Ogg audio", 2_000, "44.1 kHz", "6.9 kB"),
        ["Ogg audio", "0:02", "44.1 kHz", "6.9 kB"]
    );
    assert_eq!(facts("/m/Clip.MKV", "", 0, "", ""), ["mkv"]);
    assert!(
        facts("/m/.hidden", "", 0, "", "").is_empty(),
        "a leading dot is no suffix"
    );
    assert!(facts("/m/noext", "", -1, "", "").is_empty());
}

#[test]
fn pv3_file_uri_escapes_what_encode_uri_does_and_the_hash_and_query() {
    assert_eq!(file_uri("/m/clip.mp4"), "file:///m/clip.mp4");
    assert_eq!(file_uri("/m/a b#c?d.ogg"), "file:///m/a%20b%23c%3Fd.ogg");
    assert_eq!(file_uri("/m/é.ogg"), "file:///m/%C3%A9.ogg");
    assert_eq!(
        file_uri("/m/\u{fffd}.ogg\u{0}e92e6f6767"),
        "file:///m/%E9.ogg",
        "a raw name escapes its own bytes"
    );
    assert_eq!(
        file_uri("/m/(x)!~*';,@&=+$.mp4"),
        "file:///m/(x)!~*';,@&=+$.mp4"
    );
    assert_eq!(file_name("/a/b/c.mp4"), "c.mp4");
}
