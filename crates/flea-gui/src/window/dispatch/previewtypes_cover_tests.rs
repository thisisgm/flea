//! An audio file's cover stays in the column frame through play, pause and seek, as v0.3.12 keeps it.
use super::previewtypes_kinds_tests::Root;
use super::previewtypes_tests::{file, local, meta, open, thumbed};
use crate::{
    g2::media_preview::engine::{self, FeedSender, Playback, Request},
    window::u07_tests::read,
};
use flea::ui_logic_media_preview::frame::{Command, Event};
use gpui::{TestAppContext, VisualTestContext};
use std::{cell::RefCell, rc::Rc, time::Duration};

/// A 4 by 4 PNG, the cover the thumbnailer would have cached.
const COVER: [u8; 73] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 4, 0, 0, 0, 4, 8, 2, 0,
    0, 0, 38, 147, 9, 41, 0, 0, 0, 16, 73, 68, 65, 84, 120, 156, 99, 56, 81, 161, 1, 71, 12, 196,
    113, 0, 83, 82, 22, 129, 25, 170, 14, 207, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// An audio-only decoder that answers play and pause, so the column's player runs without a process.
#[derive(Clone, Default)]
struct Sound(Rc<RefCell<Option<FeedSender>>>, Rc<RefCell<Vec<Command>>>);
struct Playing(Sound);
impl engine::Backend for Sound {
    fn start(&self, _: &Request) -> Result<(Box<dyn Playback>, engine::Feed), String> {
        let (sender, feed) = engine::feed();
        sender.event(Event::Loaded {
            video: false,
            duration_ms: 4000,
        });
        sender.event(Event::Playing);
        *self.0.borrow_mut() = Some(sender);
        Ok((Box::new(Playing(self.clone())), feed))
    }
}
impl Playback for Playing {
    fn send(&mut self, command: Command) {
        let event = match command {
            Command::Play => Some(Event::Playing),
            Command::Pause => Some(Event::Paused),
            _ => None,
        };
        if let (Some(event), Some(feed)) = (event, (self.0).0.borrow().as_ref()) {
            feed.event(event);
        }
        (self.0).1.borrow_mut().push(command);
    }
    fn pids(&self) -> Vec<u32> {
        Vec::new()
    }
}

/// Frames and real time for the cover's decode, which runs on the background executor.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn cover_drawn(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds("column-preview-picture").is_some()
}

#[gpui::test]
fn an_mp3_cover_stays_in_the_frame_through_play_pause_and_seek(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("cover");
    let dir = root.0.to_string_lossy().into_owned();
    let cover = root.0.join("cover.png");
    std::fs::write(&cover, COVER).unwrap();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file("song.mp3", "audio-mpeg", "MP3 audio", 0o100644)];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let sound = Sound::default();
    let backend: Rc<dyn engine::Backend> = Rc::new(sound.clone());
    cx.update(|_, cx| {
        let preview = shell
            .read(cx)
            .selection_preview(0)
            .expect("the column is mounted");
        preview.update(cx, |view, _| view.set_media_backend(backend));
    });
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","ms":4000"#);
    thumbed(&shell, cx, &lines, 0, &cover.to_string_lossy());
    settle(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "audio");
    assert!(cover_drawn(cx), "the cover draws before play");
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("p");
    settle(cx);
    assert_eq!(read(&shell, cx, "columnMediaPlaying", &[]), "true");
    assert!(cover_drawn(cx), "the cover stays while it plays");
    cx.simulate_keystrokes("p");
    settle(cx);
    assert_eq!(read(&shell, cx, "columnMediaPlaying", &[]), "false");
    assert!(cover_drawn(cx), "the cover stays while paused");
    cx.simulate_keystrokes("right");
    settle(cx);
    let sent = sound.1.borrow().clone();
    assert!(
        sent.iter().any(|c| matches!(c, Command::Seek(_))),
        "right seeks: {sent:?}"
    );
    assert!(cover_drawn(cx), "the cover stays after a seek");
}

// EY-23: with no cover, the column's audio frame is one centred note, before play and while playing, as v0.3.12 draws it.
#[gpui::test]
fn an_audio_file_without_a_cover_draws_one_centred_mark(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("audiomark");
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file(
        "tone.ogg",
        "audio-x-vorbis+ogg",
        "Ogg audio",
        0o100644,
    )];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let sound = Sound::default();
    let backend: Rc<dyn engine::Backend> = Rc::new(sound.clone());
    cx.update(|_, cx| {
        let preview = shell
            .read(cx)
            .selection_preview(0)
            .expect("the column is mounted");
        preview.update(cx, |view, _| view.set_media_backend(backend));
    });
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","ms":3000"#);
    thumbed(&shell, cx, &lines, 0, "");
    settle(cx);
    assert_eq!(read(&shell, cx, "previewColumnState", &[]), "audio");
    let mark = |cx: &mut VisualTestContext| cx.debug_bounds("column-audio-mark");
    assert!(
        mark(cx).is_some(),
        "the note stands in the frame before play"
    );
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("p");
    settle(cx);
    assert_eq!(read(&shell, cx, "columnMediaPlaying", &[]), "true");
    assert!(mark(cx).is_some(), "the note stays while it plays");
}

/// A decoder that cannot start, so the column's player fails at the first press of play.
struct Broken;
impl engine::Backend for Broken {
    fn start(&self, _: &Request) -> Result<(Box<dyn Playback>, engine::Feed), String> {
        Err("no decoder".into())
    }
}

// EY-23 review: a player that failed draws its failure in the column, never the playable note.
#[gpui::test]
fn a_failed_audio_player_draws_its_failure_not_the_note(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let root = Root::new("audiofail");
    let dir = root.0.to_string_lossy().into_owned();
    let (shell, cx) = open(cx, &dir);
    let rows = vec![file(
        "tone.ogg",
        "audio-x-vorbis+ogg",
        "Ogg audio",
        0o100644,
    )];
    super::test_shell::land(&shell, cx, &dir, rows, 1);
    let backend: Rc<dyn engine::Backend> = Rc::new(Broken);
    cx.update(|_, cx| {
        let preview = shell
            .read(cx)
            .selection_preview(0)
            .expect("the column is mounted");
        preview.update(cx, |view, _| view.set_media_backend(backend));
    });
    let lines = local(&shell, cx, &dir);
    meta(&shell, cx, &lines, r#","ms":3000"#);
    thumbed(&shell, cx, &lines, 0, "");
    settle(cx);
    assert!(cx.debug_bounds("column-audio-mark").is_some());
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("p");
    settle(cx);
    assert!(
        cx.debug_bounds("column-audio-mark").is_none(),
        "the failure replaces the note"
    );
    assert!(
        cx.debug_bounds("column-media-failed").is_some(),
        "the player's failure picture draws in the frame"
    );
}
