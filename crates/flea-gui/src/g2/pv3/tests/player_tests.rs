use super::fake::{picture, Fake, Script};
use crate::g2::pv3::player::{Inhibit, MediaPlayer, Subject};
use crate::g2::pv3::sound::MediaSound;
use crate::g2::pv3::strip::StripInput;
use crate::g2::pv3::surface::{picture as shown, Facts};
use crate::g2::pv3::view::Picture;
use flea::ui_logic_pv3::frame::{Command, Event, Failure};
use flea::ui_logic_pv3::player::{MediaKey, Status};
use gpui::{AppContext, Entity, TestAppContext};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

pub(super) fn subject(name: &str, video: bool) -> Subject {
    Subject {
        path: PathBuf::from("/m").join(name),
        video,
        box_w: 902,
        box_h: 547,
        autoplay: true,
    }
}

pub(super) type Awake = Rc<RefCell<Vec<bool>>>;

pub(super) fn player(
    cx: &mut TestAppContext,
    script: &Rc<Script>,
    program: Option<PathBuf>,
) -> (Entity<MediaPlayer>, Awake) {
    let awake: Awake = Rc::default();
    let seen = awake.clone();
    let inhibit: Inhibit = Rc::new(move |on, _| seen.borrow_mut().push(on));
    let backend = Rc::new(Fake(script.clone()));
    let player = cx.new(|cx| {
        let mut player = MediaPlayer::new(backend, Some(inhibit), cx);
        player.mpris_program = program.unwrap_or_else(|| "/nonexistent/flea-mpris".into());
        player
    });
    (player, awake)
}

pub(super) fn send(cx: &mut TestAppContext, script: &Script, event: Event) {
    script.sender().event(event);
    cx.run_until_parked();
}

#[gpui::test]
fn pv3_a_video_starts_with_the_shared_sound_and_holds_the_screen_only_while_playing(
    cx: &mut TestAppContext,
) {
    let script = Rc::new(Script::default());
    let (player, awake) = player(cx, &script, None);
    player.update(cx, |player, cx| player.open(subject("clip.mp4", true), cx));
    let request = script.requests.borrow()[0].clone();
    assert_eq!(
        (
            request.box_w,
            request.box_h,
            request.autoplay,
            request.audio
        ),
        (902, 547, true, true)
    );
    assert_eq!(
        script.take_commands(),
        [Command::Mute(false), Command::Gain(1.)]
    );
    assert_eq!(
        player.read_with(cx, |player, _| player.status()),
        Status::Loading
    );
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 2_000,
        },
    );
    send(cx, &script, Event::Playing);
    assert_eq!(
        player.read_with(cx, |player, _| player.status()),
        Status::Playing
    );
    send(cx, &script, Event::Paused);
    assert_eq!(
        *awake.borrow(),
        [true, false],
        "the inhibitor follows a playing picture only"
    );
    player.update(cx, |player, cx| player.close(cx));
    assert_eq!(script.dropped.get(), 1, "closing ends the decoder");
    assert_eq!(
        player.read_with(cx, |player, _| player.status()),
        Status::Stopped
    );
}

#[gpui::test]
fn pv3_audio_never_holds_the_screen_awake(cx: &mut TestAppContext) {
    let script = Rc::new(Script::default());
    let (player, awake) = player(cx, &script, None);
    player.update(cx, |player, cx| player.open(subject("tone.ogg", false), cx));
    send(
        cx,
        &script,
        Event::Loaded {
            video: false,
            duration_ms: 2_000,
        },
    );
    send(cx, &script, Event::Playing);
    assert!(awake.borrow().is_empty());
}

// Each subject is a generation: a late record from the last one is ignored, and its picture stays up until the next draws.
#[gpui::test]
fn pv3_the_previous_picture_holds_until_the_next_subject_draws(cx: &mut TestAppContext) {
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    player.update(cx, |player, cx| player.open(subject("a.mp4", true), cx));
    let first = script.sender();
    let a = picture(1);
    first.frame(a.clone());
    cx.run_until_parked();
    player.update(cx, |player, cx| player.open(subject("b.mp4", true), cx));
    assert_eq!(script.dropped.get(), 1, "the first decoder ended");
    let late = picture(9);
    first.frame(late);
    first.event(Event::Playing);
    cx.run_until_parked();
    let (frame, generation, current, status) = player.read_with(cx, |player, _| {
        (
            player.frame.as_ref().map(|image| image.id),
            player.frame_generation,
            player.generation(),
            player.status(),
        )
    });
    assert_eq!(
        frame,
        Some(a.id),
        "the old picture holds and the late one is dropped"
    );
    assert!(
        generation < current,
        "it is known as the previous subject's"
    );
    assert_eq!(
        status,
        Status::Loading,
        "the late Playing never reached the new subject"
    );
    let b = picture(2);
    script.sender().frame(b.clone());
    cx.run_until_parked();
    let (frame, retired) = player.read_with(cx, |player, _| {
        (
            player.frame.as_ref().map(|image| image.id),
            player.painted.borrow().retired.is_some(),
        )
    });
    assert_eq!(frame, Some(b.id));
    assert!(!retired, "a picture no paint uploaded goes at once");
}

#[gpui::test]
fn pv3_a_failure_shows_the_sentence_and_ends_the_decoder(cx: &mut TestAppContext) {
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    player.update(cx, |player, cx| player.open(subject("bad.mp4", true), cx));
    send(cx, &script, Event::Failed(Failure::Unsupported));
    assert_eq!(script.dropped.get(), 1);
    let catalog = crate::test_catalogs::english();
    let picture = player.read_with(cx, |player, _| shown(player, &Facts::default(), &catalog));
    assert_eq!(
        picture,
        Picture::Failed {
            sentence: catalog.tr("media.unplayable")
        }
    );
    let refused = Rc::new(Script::default());
    refused.refuse.set(true);
    let (player, _) = self::player(cx, &refused, None);
    player.update(cx, |player, cx| player.open(subject("x.mp4", true), cx));
    assert_eq!(
        player.read_with(cx, |player, _| player.status()),
        Status::Failed,
        "a decoder that cannot start is a failure"
    );
}

// MediaStrip and MediaVolume: a scrub seeks once on release, the wheel steps a second, a level is kept once.
#[gpui::test]
fn pv3_strip_input_seeks_on_release_and_keeps_the_level_once(cx: &mut TestAppContext) {
    let kept: Rc<RefCell<Vec<u8>>> = Rc::default();
    let record = kept.clone();
    cx.update(|cx| {
        MediaSound::install(
            80,
            Some(Rc::new(move |level, _| record.borrow_mut().push(level))),
            cx,
        )
    });
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, None);
    player.update(cx, |player, cx| player.open(subject("clip.mp4", true), cx));
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 10_000,
        },
    );
    send(cx, &script, Event::Playing);
    script.take_commands();
    let window = cx.add_empty_window();
    let input = |inputs: &[StripInput], window: &mut gpui::VisualTestContext| {
        for input in inputs {
            window.update(|window, cx| {
                player.update(cx, |player, cx| player.strip_input(*input, window, cx))
            });
        }
    };
    input(
        &[
            StripInput::ScrubPress(0.2),
            StripInput::ScrubMove(0.4),
            StripInput::ScrubMove(0.5),
        ],
        window,
    );
    let view = player.read_with(window, |player, cx| player.strip_view(false, cx));
    assert_eq!(
        view.scrub,
        Some(0.5),
        "the track draws from the pointer while dragging"
    );
    assert!(script.take_commands().is_empty(), "no seek while dragging");
    input(&[StripInput::ScrubRelease], window);
    assert_eq!(script.take_commands(), [Command::Seek(5_000)]);
    input(&[StripInput::Wheel { up: true }], window);
    assert_eq!(script.take_commands(), [Command::Seek(6_000)]);
    input(
        &[StripInput::LevelPress(30), StripInput::LevelMove(40)],
        window,
    );
    window.run_until_parked();
    assert!(kept.borrow().is_empty(), "a press writes nothing");
    assert_eq!(
        player.read_with(window, |player, cx| player.strip_view(false, cx).level),
        40
    );
    input(&[StripInput::LevelRelease], window);
    window.run_until_parked();
    assert_eq!(*kept.borrow(), [40]);
    let commands = script.take_commands();
    assert!(
        commands.contains(&Command::Gain(flea::ui_logic_pv3::gain(40))),
        "{commands:?}"
    );
    input(&[StripInput::Toggle], window);
    assert_eq!(script.take_commands(), [Command::Pause]);
    player.update(window, |player, cx| player.key(MediaKey::Seek(-9_000), cx));
    assert_eq!(
        script.take_commands(),
        [Command::Seek(0)],
        "a seek clamps to the clip"
    );
    player.update(window, |player, cx| player.key(MediaKey::Mute, cx));
    window.run_until_parked();
    assert!(window.update(|_, cx| MediaSound::get(cx).muted));
    assert!(script.take_commands().contains(&Command::Mute(true)));
}
