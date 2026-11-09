use super::player::*;
use super::session::*;

fn playing(video: bool) -> Player {
    Player {
        playback: Playback::Playing,
        media: Media::Buffered,
        failed: false,
        has_video: video,
    }
}

#[test]
fn pv3_status_reads_a_failure_first_then_playback_then_loading() {
    let mut player = Player::default();
    assert_eq!(player.status(), Status::Stopped);
    player.media = Media::Loading;
    assert_eq!(player.status(), Status::Loading);
    player.media = Media::Buffering;
    assert_eq!(player.status(), Status::Loading);
    player.playback = Playback::Paused;
    assert_eq!(player.status(), Status::Paused);
    player.playback = Playback::Playing;
    assert_eq!(player.status(), Status::Playing);
    player.failed = true;
    assert_eq!(player.status(), Status::Failed);
    assert_eq!(Status::Failed.word(), "failed");
}

#[test]
fn pv3_the_poster_mark_waits_for_a_decoded_stopped_video() {
    let mut player = Player::default();
    assert!(
        !player.poster(true),
        "construction is Stopped, yet nothing has decoded"
    );
    for media in [Media::Loaded, Media::Buffered, Media::Ended] {
        player.media = media;
        assert!(player.poster(true));
        assert!(!player.poster(false), "audio never draws the play mark");
    }
    player.playback = Playback::Playing;
    assert!(!player.poster(true));
    player.playback = Playback::Paused;
    player.failed = true;
    assert!(!player.poster(true));
}

// tests/mediasession.qml: the inhibitor follows a playing picture only.
#[test]
fn pv3_only_a_playing_video_keeps_the_screen_awake() {
    assert!(playing(true).keeps_awake());
    assert!(
        !playing(false).keeps_awake(),
        "audio playing: the inhibitor stays off"
    );
    let mut paused = playing(true);
    paused.playback = Playback::Paused;
    assert!(!paused.keeps_awake());
    paused.playback = Playback::Stopped;
    assert!(!paused.keeps_awake());
    let mut failed = playing(true);
    failed.failed = true;
    assert!(
        !failed.keeps_awake(),
        "a failed decoder never holds the screen"
    );
}

fn state(playback: Playback, position: i64, seeked: bool) -> LineState {
    LineState {
        playback,
        path: "/m/clip.mp4".into(),
        length: 2_000,
        position,
        seeked,
    }
}

// stepBefore and stepFirstLine: no helper line before play, then Playing with the name and its url.
#[test]
fn pv3_the_session_is_silent_until_the_first_play() {
    let mut session = Session::default();
    assert_eq!(session.line(&state(Playback::Stopped, 0, false)), None);
    session.playback_changed(Playback::Paused);
    assert!(!session.active);
    session.playback_changed(Playback::Playing);
    let line = session.line(&state(Playback::Playing, 0, false)).unwrap();
    assert_eq!(
        line,
        r#"{"status":"Playing","title":"clip.mp4","url":"file:///m/clip.mp4","length":2000,"position":0,"seeked":false}"#
    );
    session.playback_changed(Playback::Stopped);
    assert!(session.active, "a stop keeps the player on the bus");
}

#[test]
fn pv3_a_state_line_escapes_a_quoted_name() {
    let line = LineState {
        path: "/m/a \"b\".ogg".into(),
        ..state(Playback::Paused, 1_500, true)
    }
    .render();
    assert!(line.contains(r#""title":"a \"b\".ogg""#), "{line}");
    assert!(
        line.contains(r#""status":"Paused""#)
            && line.ends_with(r#""position":1500,"seeked":true}"#)
    );
}

// The helper's commands: Seek relative, SetPosition absolute, everything else refused.
#[test]
fn pv3_remote_lines_parse_and_malformed_ones_are_refused() {
    assert_eq!(Remote::parse("Play"), Some(Remote::Play));
    assert_eq!(Remote::parse("Pause"), Some(Remote::Pause));
    assert_eq!(Remote::parse("PlayPause"), Some(Remote::PlayPause));
    assert_eq!(Remote::parse("Stop"), Some(Remote::Stop));
    assert_eq!(Remote::parse("Seek -2500"), Some(Remote::Seek(-2500)));
    assert_eq!(Remote::parse("Seek 300"), Some(Remote::Seek(300)));
    assert_eq!(
        Remote::parse("SetPosition 700"),
        Some(Remote::SetPosition(700))
    );
    for bad in [
        "",
        "Seek",
        "Seek x",
        "SetPosition",
        "SetPosition 1e3",
        "Rewind",
        "play",
    ] {
        assert_eq!(Remote::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn pv3_seeks_clamp_to_the_clip() {
    assert_eq!(clamp_seek(1_500, 2_000), 1_500);
    assert_eq!(clamp_seek(-10, 2_000), 0);
    assert_eq!(clamp_seek(9_000, 2_000), 2_000);
    assert_eq!(clamp_seek(700, 0), 0);
}

// tests/js/previewkeys.js: p plays only where there is something to play, m mutes, the arrows seek 5 s.
#[test]
fn pv3_media_keys_self_guard_on_a_still_preview() {
    assert_eq!(media_key("playPause", true), Some(MediaKey::TogglePlay));
    assert_eq!(media_key("playPause", false), None);
    assert_eq!(media_key("mute", true), Some(MediaKey::Mute));
    assert_eq!(media_key("mute", false), None);
    assert_eq!(media_key("seekBack", true), Some(MediaKey::Seek(-5_000)));
    assert_eq!(media_key("seekForward", true), Some(MediaKey::Seek(5_000)));
    assert_eq!(
        media_key("preview", true),
        None,
        "space closes, it never plays"
    );
    assert_eq!(media_key("escape", true), None);
}

// PreviewColumn: no player until play, and the intent ends with the subject, the column or an overlay.
#[test]
fn pv3_the_column_builds_a_player_only_on_play() {
    let mut column = ColumnPlay::default();
    column.set_allowed(true, false, true);
    assert!(!column.player(), "browsing builds no player");
    assert_eq!(column.press(false), ColumnPress::Create);
    assert!(column.player());
    assert_eq!(column.press(true), ColumnPress::Toggle);
    assert_eq!(column.press(false), ColumnPress::Nothing);
    column.path_changed();
    assert!(!column.player(), "a new row is a new subject");
    column.press(false);
    column.set_allowed(true, true, true);
    assert!(!column.wants, "an open overlay ends the intent");
    column.set_allowed(true, false, true);
    assert!(!column.player(), "and it does not come back by itself");
    column.press(false);
    column.set_allowed(false, false, true);
    assert!(!column.player(), "a hidden column ends it");
    column.set_allowed(true, false, true);
    column.press(false);
    column.set_allowed(true, false, false);
    assert!(!column.player(), "no strip, no player");
}

#[test]
fn pv3_the_column_strip_scales_from_the_probe_until_the_player_plays() {
    assert_eq!(column_duration(0, 0, 2_000), 2_000);
    assert_eq!(column_duration(0, 2_100, 2_000), 2_000);
    assert_eq!(column_duration(10, 2_100, 2_000), 2_100);
    assert_eq!(column_duration(10, 0, -4), 0);
}

// mediastrip-volume.qml's hold legs: a held press keeps the strip past its delay, a release lets it go.
#[test]
fn pv3_the_quick_look_strip_hides_after_its_delay_unless_held() {
    let mut hide = StripHide::default();
    hide.hide_ms = 60;
    assert!(hide.shown && hide.deadline().is_none());
    hide.reveal(1_000);
    hide.tick(1_059, false);
    assert!(hide.shown);
    hide.tick(1_060, true);
    assert!(hide.shown, "still shown past the delay under a held press");
    assert_eq!(hide.deadline(), Some(1_120));
    hide.tick(1_120, false);
    assert!(!hide.shown, "hides once the press is released");
    assert!(hide.visible(true), "audio's strip never hides");
    assert!(!hide.visible(false));
    hide.reveal(2_000);
    hide.stop();
    hide.tick(9_000, false);
    assert!(hide.shown, "a stopped timer hides nothing");
    assert_eq!(StripHide::default().hide_ms, 4_000);
}

#[test]
fn an_ended_clip_shows_its_play_mark_until_it_plays_again() {
    let mut p = Player {
        has_video: true,
        media: Media::Buffered,
        playback: Playback::Playing,
        failed: false,
    };
    assert!(!p.poster(true), "a playing clip has no play mark");
    p.playback = Playback::Paused;
    assert!(p.poster(true), "a paused clip shows its play mark");
    (p.media, p.playback) = (Media::Ended, Playback::Stopped);
    assert!(
        p.status() == Status::Stopped && p.poster(true),
        "the ended clip is stopped under its play mark"
    );
    (p.media, p.playback) = (Media::Buffered, Playback::Paused);
    assert!(
        p.poster(true),
        "a replay paused mid-clip shows its play mark"
    );
    p.playback = Playback::Playing;
    assert!(!p.poster(true), "a replay hides its play mark");
}
