use super::fake::{picture, Script};
use super::player_tests::{player, send, subject};
use crate::g2::media_preview::surface::{column_view, look_view, picture as shown, Facts};
use crate::g2::media_preview::view::Picture;
use flea::ui_logic_media_preview::frame::{Command, Event};
use flea::ui_logic_media_preview::player::Status;
use gpui::TestAppContext;
use std::rc::Rc;

fn video_end(cx: &mut TestAppContext, column: bool) {
    let script = Rc::new(Script::default());
    let (player, awake) = player(cx, &script, None);
    let catalog = crate::test_catalogs::english();
    player.update(cx, |player, cx| {
        player.rewind_on_end = column;
        player.open(subject("clip.mp4", true), cx);
    });
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 3_000,
        },
    );
    let poster = picture(1);
    script.sender().frame(poster.clone());
    send(cx, &script, Event::Playing);
    let last = picture(2);
    script.sender().frame(last.clone());
    send(cx, &script, Event::Position(2_900));
    script.take_commands();
    send(cx, &script, Event::Ended);
    assert_eq!(
        script.take_commands(),
        if column { vec![Command::Stop] } else { vec![] },
        "only the column rewinds; Quick Look holds its ended frame"
    );
    send(cx, &script, Event::Position(3_000));
    player.read_with(cx, |player, cx| {
        let strip = if column {
            column_view(Some(player), 0, cx)
        } else {
            player.strip_view(false, cx)
        };
        assert_eq!(
            (strip.position, strip.duration, strip.playing),
            (if column { 0 } else { 3_000 }, 3_000, false)
        );
        assert_eq!(
            shown(player, &Facts::default(), &catalog),
            Picture::Video {
                frame: Some(last),
                poster: true
            },
            "the current picture holds until the rewind draws"
        );
    });
    if !column {
        assert_eq!(*awake.borrow(), [true, false]);
        return;
    }
    // Stop rewinds the real decoder without a user Seeked event; its first frame replaces the held picture.
    send(cx, &script, Event::Stopped);
    script.sender().frame(poster.clone());
    send(cx, &script, Event::Position(0));
    player.read_with(cx, |player, cx| {
        let strip = if column {
            column_view(Some(player), 0, cx)
        } else {
            player.strip_view(false, cx)
        };
        assert_eq!(
            (strip.position, strip.duration, strip.playing),
            (0, 3_000, false)
        );
        assert_eq!(
            shown(player, &Facts::default(), &catalog),
            Picture::Video {
                frame: Some(poster),
                poster: true
            }
        );
        assert_eq!(player.status(), Status::Stopped);
    });
    assert_eq!(*awake.borrow(), [true, false]);
    assert_eq!(
        script.dropped.get(),
        0,
        "the stopped decoder stays available for replay"
    );
}

#[gpui::test]
fn vf4_quick_look_video_end_holds_last_frame_and_duration(cx: &mut TestAppContext) {
    video_end(cx, false);
}

// EY-52: ql-video.png and v0.3.12 show an ended clip as the empty card under the play mark at 0:02 / 0:02.
#[gpui::test]
fn pv3_quick_look_video_end_shows_the_empty_card_at_full_length(cx: &mut TestAppContext) {
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    let catalog = crate::test_catalogs::english();
    player.update(cx, |player, cx| player.open(subject("clip.mp4", true), cx));
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 2_000,
        },
    );
    script.sender().frame(picture(1));
    send(cx, &script, Event::Playing);
    script.sender().frame(picture(2));
    send(cx, &script, Event::Position(1_960));
    send(cx, &script, Event::Ended);
    player.read_with(cx, |player, cx| {
        let (picture, strip) = look_view(player, &Facts::default(), &catalog, cx);
        assert_eq!(
            picture,
            Picture::Video {
                frame: None,
                poster: true
            },
            "no frame under the play mark"
        );
        assert_eq!(
            (strip.position, strip.duration, strip.playing),
            (2_000, 2_000, false),
            "the strip reads the whole clip"
        );
    });
}

#[gpui::test]
fn pv3_columns_video_end_returns_to_poster_and_zero_clock(cx: &mut TestAppContext) {
    video_end(cx, true);
}

fn replay(cx: &mut TestAppContext, column: bool) {
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    let catalog = crate::test_catalogs::english();
    player.update(cx, |player, cx| {
        player.rewind_on_end = column;
        player.open(subject("clip.mp4", true), cx);
    });
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 3_000,
        },
    );
    send(cx, &script, Event::Playing);
    send(cx, &script, Event::Ended);
    script.take_commands();
    player.update(cx, |player, _| player.seek_to(1_000));
    assert_eq!(script.take_commands(), [Command::Seek(1_000)]);
    let sought = picture(2);
    script.sender().frame(sought.clone());
    send(cx, &script, Event::Seeked(1_000));
    player.read_with(cx, |player, _| {
        assert_eq!(player.position, 1_000);
        assert_eq!(
            shown(player, &Facts::default(), &catalog),
            Picture::Video {
                frame: Some(sought),
                poster: true
            }
        );
    });
    send(cx, &script, Event::Ended);
    if column {
        // The column rewinds at the end; the helper answers its Stop with Stopped.
        assert_eq!(script.take_commands(), [Command::Stop]);
        send(cx, &script, Event::Stopped);
    }
    player.update(cx, |player, cx| player.toggle(cx));
    assert_eq!(script.take_commands(), [Command::Play]);
    if !column {
        // Quick Look holds the end; the helper's Play there seeks to 0 first and reports it.
        send(cx, &script, Event::Seeked(0));
    }
    send(cx, &script, Event::Playing);
    let replay = picture(3);
    script.sender().frame(replay.clone());
    send(cx, &script, Event::Position(10));
    player.read_with(cx, |player, _| {
        assert_eq!(player.position, 10);
        assert_eq!(
            shown(player, &Facts::default(), &catalog),
            Picture::Video {
                frame: Some(replay.clone()),
                poster: false
            }
        );
    });
    send(cx, &script, Event::Paused);
    player.read_with(cx, |player, _| {
        assert_eq!(player.position, 10);
        assert_eq!(
            shown(player, &Facts::default(), &catalog),
            Picture::Video {
                frame: Some(replay),
                poster: true
            }
        );
    });
}

#[gpui::test]
fn pv3_video_end_still_allows_seeking_replay_and_pause(cx: &mut TestAppContext) {
    replay(cx, false);
}

#[gpui::test]
fn vf4_r1_column_video_replays_after_its_end_rewind(cx: &mut TestAppContext) {
    replay(cx, true);
}

#[gpui::test]
fn pv3_audio_end_keeps_its_existing_clock_and_transport(cx: &mut TestAppContext) {
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    player.update(cx, |player, cx| player.open(subject("tone.ogg", false), cx));
    send(
        cx,
        &script,
        Event::Loaded {
            video: false,
            duration_ms: 3_000,
        },
    );
    send(cx, &script, Event::Playing);
    script.take_commands();
    send(cx, &script, Event::Ended);
    assert!(script.take_commands().is_empty());
    assert_eq!(
        player.read_with(cx, |player, _| (player.position, player.status())),
        (3_000, Status::Stopped)
    );
}

#[gpui::test]
fn pv3_columns_ended_video_draws_its_film_strip_still_again(cx: &mut TestAppContext) {
    use crate::window::selection_preview::render::column_media;
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    let catalog = crate::test_catalogs::english();
    // The cached thumbnail carries the film-strip edges; the decoder's own frames never do.
    let still = picture(9);
    player.update(cx, |player, cx| player.open(subject("clip.mp4", true), cx));
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 3_000,
        },
    );
    send(cx, &script, Event::Playing);
    let playing = picture(2);
    script.sender().frame(playing.clone());
    send(cx, &script, Event::Position(1_000));
    player.read_with(cx, |player, _| {
        assert_eq!(
            column_media(Some(player), Some(still.clone()), &catalog),
            Picture::Video {
                frame: Some(playing),
                poster: false
            },
            "a playing clip draws the decoder's frame"
        );
    });
    send(cx, &script, Event::Ended);
    send(cx, &script, Event::Stopped);
    script.sender().frame(picture(1));
    send(cx, &script, Event::Position(0));
    player.read_with(cx, |player, _| {
        assert_eq!(
            column_media(Some(player), Some(still.clone()), &catalog),
            Picture::Video {
                frame: Some(still.clone()),
                poster: false
            },
            "an ended clip returns to the rest poster, not the rewound first frame"
        );
        assert_eq!(
            column_media(None, Some(still.clone()), &catalog),
            column_media(Some(player), Some(still.clone()), &catalog),
            "the ended picture is the one drawn before Play"
        );
    });
}
