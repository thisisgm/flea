use super::fake::{picture, Script};
use super::player_tests::{player, send, subject};
use crate::g2::pv3::surface::{column_view, picture as shown, Facts};
use crate::g2::pv3::view::Picture;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_pv3::frame::{Command, Event};
use flea::ui_logic_pv3::player::Status;
use gpui::TestAppContext;
use std::rc::Rc;

fn video_end(cx: &mut TestAppContext, column: bool) {
    let script = Rc::new(Script::default());
    let (player, awake) = player(cx, &script, None);
    let catalog = I18n::load(Paths::default());
    player.update(cx, |player, cx| player.open(subject("clip.mp4", true), cx));
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
        [Command::Stop],
        "the decoder must rewind while stopped"
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
            (0, 3_000, false)
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
fn pv3_quick_look_video_end_returns_to_poster_and_zero_clock(cx: &mut TestAppContext) {
    video_end(cx, false);
}

#[gpui::test]
fn pv3_columns_video_end_returns_to_poster_and_zero_clock(cx: &mut TestAppContext) {
    video_end(cx, true);
}

#[gpui::test]
fn pv3_video_end_still_allows_seeking_replay_and_pause(cx: &mut TestAppContext) {
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    let catalog = I18n::load(Paths::default());
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
    send(cx, &script, Event::Stopped);
    player.update(cx, |player, cx| player.toggle(cx));
    assert_eq!(script.take_commands(), [Command::Stop, Command::Play]);
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
