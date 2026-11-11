//! Media kinds mount a player through the real Shell, and a click on a video's picture toggles it as p does (#424).
use super::preview_tests::{close, context, draw, file, opened};
use super::previewtypes_tests as column;
use super::quick_look_body_tests::{mounted, open_kind, Fake, Script};
use super::test_shell::{land, set_cursor};
use crate::state::Shell;
use crate::window::u07_tests::read;
use flea::keys::Context;
use flea::ui_logic_media_preview::frame::{Command, Event, Failure};
use gpui::{Entity, Pixels, VisualTestContext};
use gpui::{Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Point, TestAppContext};
use std::rc::Rc;

fn media(cx: &mut TestAppContext, name: &str, kind: &str) {
    cx.executor().allow_parking();
    let (shell, cx, _requests) = opened(cx);
    land(
        &shell,
        cx,
        "/files",
        vec![file(name, &format!("{kind}-generic"))],
        1,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewKind", &[]),
        kind,
        "{name} mounts media"
    );
    assert_eq!(context(&shell, cx), Context::Media);
    assert!(
        cx.debug_bounds("pv1-media").is_some(),
        "{name} draws its player"
    );
    cx.simulate_keystrokes("space");
    draw(cx);
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "false");
    close(&shell, cx);
}

macro_rules! kinds {
    ($($test:ident => ($name:literal, $kind:literal),)*) => {$(
        #[gpui::test]
        fn $test(cx: &mut TestAppContext) {
            media(cx, $name, $kind);
        }
    )*};
}

kinds! {
    mp4_mounts_media => ("clip.mp4", "video"),
    mkv_mounts_media => ("clip.mkv", "video"),
    webm_mounts_media => ("clip.webm", "video"),
    mov_mounts_media => ("clip.mov", "video"),
    mp3_mounts_media => ("song.mp3", "audio"),
    ogg_mounts_media => ("song.ogg", "audio"),
    opus_mounts_media => ("song.opus", "audio"),
    flac_mounts_media => ("song.flac", "audio"),
    wav_mounts_media => ("song.wav", "audio"),
    m4a_mounts_media => ("song.m4a", "audio"),
}

/// The decoder's play and pause commands since the last read; the opening mute and gain are not a press.
fn toggles(script: &Script) -> Vec<Command> {
    std::mem::take(&mut *script.commands.borrow_mut())
        .into_iter()
        .filter(|command| matches!(command, Command::Play | Command::Pause))
        .collect()
}

/// A double-click as the platform reports it: the second press and release carry a click count of 2.
fn double_click(cx: &mut VisualTestContext, at: Point<Pixels>) {
    for clicks in [1, 2] {
        cx.simulate_event(MouseDownEvent {
            position: at,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count: clicks,
            first_mouse: false,
        });
        cx.simulate_event(MouseUpEvent {
            position: at,
            modifiers: Modifiers::none(),
            button: MouseButton::Left,
            click_count: clicks,
        });
    }
}

// #424: a click on the playing picture in Quick Look pauses it and the next plays it, as p does; a double-click toggles once.
#[gpui::test]
fn a_click_on_the_quick_look_picture_plays_and_pauses_as_p_does(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "clip.mp4");
    let feed = |event| assert!(script.feeds.borrow()[0].event(event));
    feed(Event::Loaded {
        video: true,
        duration_ms: 20_000,
    });
    feed(Event::Playing);
    draw(cx);
    assert_eq!(read(&shell, cx, "previewState", &[]), "playing");
    toggles(&script);
    let picture = cx
        .debug_bounds("pv1-media")
        .expect("the card draws its player")
        .center();
    cx.simulate_click(picture, Modifiers::none());
    assert_eq!(toggles(&script), [Command::Pause], "the click pauses");
    feed(Event::Paused);
    draw(cx);
    assert_eq!(read(&shell, cx, "previewState", &[]), "paused");
    cx.simulate_click(picture, Modifiers::none());
    assert_eq!(toggles(&script), [Command::Play], "the next click plays");
    feed(Event::Playing);
    draw(cx);
    double_click(cx, picture);
    assert_eq!(
        toggles(&script),
        [Command::Pause],
        "a double-click toggles once"
    );
    feed(Event::Paused);
    draw(cx);
    cx.simulate_keystrokes("p");
    assert_eq!(toggles(&script), [Command::Play], "p takes the same path");
    assert_eq!(
        read(&shell, cx, "previewOpen", &[]),
        "true",
        "a click inside the card never closes it"
    );
    close(&shell, cx);
}

// #424: a press while the card is still held back only reveals it, as the ground rules, and toggles nothing.
#[gpui::test]
fn a_press_during_the_quick_look_hold_only_reveals_the_card(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "clip.mp4");
    let feed = |event| assert!(script.feeds.borrow()[0].event(event));
    feed(Event::Loaded {
        video: true,
        duration_ms: 20_000,
    });
    feed(Event::Playing);
    draw(cx);
    toggles(&script);
    let picture = cx
        .debug_bounds("pv1-media")
        .expect("the card draws its player")
        .center();
    let look = shell.read_with(cx, |shell, _| {
        shell.quick_look.as_ref().unwrap().view.clone()
    });
    look.update(cx, |look, cx| {
        look.revealed = false;
        cx.notify();
    });
    draw(cx);
    cx.simulate_click(picture, Modifiers::none());
    draw(cx);
    assert!(
        toggles(&script).is_empty(),
        "the held press toggles nothing"
    );
    assert!(
        look.read_with(cx, |look, _| look.is_revealed()),
        "the press revealed the card"
    );
    cx.simulate_click(picture, Modifiers::none());
    assert_eq!(
        toggles(&script),
        [Command::Pause],
        "the revealed card takes the next click"
    );
    close(&shell, cx);
}

/// The column showing one video's poster through the scripted decoder, and the centre of its frame.
fn column_clip(
    cx: &mut TestAppContext,
) -> (
    Entity<Shell>,
    &mut VisualTestContext,
    Rc<Script>,
    Point<Pixels>,
) {
    let (shell, cx) = column::open(cx, "/files");
    let rows = vec![column::file(
        "clip.mp4",
        "video-mp4",
        "MPEG-4 video",
        0o100644,
    )];
    land(&shell, cx, "/files", rows, 1);
    let lines = column::local(&shell, cx, "/files");
    column::meta(&shell, cx, &lines, r#","ms":20000"#);
    let preview = shell.read_with(cx, |shell, _| shell.selection_preview(0).unwrap());
    let script = Rc::new(Script::default());
    preview.update(cx, |view, _| {
        view.set_media_backend(Rc::new(Fake(script.clone())));
        let frame = view.pending.as_mut().or(view.drawn.as_mut()).unwrap();
        frame.thumb_answered = true;
        frame.source = None;
    });
    column::pump(cx);
    let frame = cx
        .debug_bounds("column-preview-frame")
        .expect("the column draws the video's frame")
        .center();
    (shell, cx, script, frame)
}

#[gpui::test]
fn fx_uish_def3_quick_look_releases_the_columns_player(cx: &mut TestAppContext) {
    let (shell, cx, script, frame) = column_clip(cx);
    shell.update_in(cx, |shell, window, cx| {
        shell.quick_look_host(0, window, cx).backend = Some(Rc::new(Fake(script)));
    });
    cx.simulate_click(frame, Modifiers::none());
    column::pump(cx);
    assert_eq!(read(&shell, cx, "columnPlayerLoaded", &[]), "true");
    cx.simulate_keystrokes("space");
    column::pump(cx);
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    assert_eq!(read(&shell, cx, "columnPlayerLoaded", &[]), "false");
    cx.simulate_keystrokes("escape");
    column::pump(cx);
    assert_eq!(read(&shell, cx, "columnPlayerLoaded", &[]), "false");
    close(&shell, cx);
}

// #424: a click on the column's video frame starts, pauses and plays it as p does, and the listing keeps the keyboard.
#[gpui::test]
fn a_click_on_the_column_video_frame_plays_and_pauses_as_p_does(cx: &mut TestAppContext) {
    let (shell, cx, script, frame) = column_clip(cx);
    cx.simulate_click(frame, Modifiers::none());
    column::pump(cx);
    assert_eq!(
        read(&shell, cx, "columnPlayerLoaded", &[]),
        "true",
        "a click on the poster starts the player"
    );
    let feed = |event| assert!(script.feeds.borrow()[0].event(event));
    feed(Event::Loaded {
        video: true,
        duration_ms: 20_000,
    });
    feed(Event::Playing);
    column::pump(cx);
    assert_eq!(read(&shell, cx, "columnMediaPlaying", &[]), "true");
    toggles(&script);
    cx.simulate_click(frame, Modifiers::none());
    column::pump(cx);
    assert_eq!(toggles(&script), [Command::Pause], "the click pauses");
    feed(Event::Paused);
    column::pump(cx);
    assert_eq!(read(&shell, cx, "columnMediaPlaying", &[]), "false");
    double_click(cx, frame);
    column::pump(cx);
    assert_eq!(
        toggles(&script),
        [Command::Play],
        "a double-click toggles once"
    );
    feed(Event::Playing);
    column::pump(cx);
    assert_eq!(
        context(&shell, cx),
        Context::Listing,
        "a click on the frame leaves the listing the keyboard"
    );
    cx.simulate_keystrokes("ctrl-tab");
    cx.simulate_keystrokes("p");
    column::pump(cx);
    assert_eq!(toggles(&script), [Command::Pause], "p takes the same path");
}

// #424: a click on a failed column video does what p does there, which is nothing.
#[gpui::test]
fn a_click_on_a_failed_column_video_starts_and_sends_nothing(cx: &mut TestAppContext) {
    let (_shell, cx, script, frame) = column_clip(cx);
    cx.simulate_click(frame, Modifiers::none());
    column::pump(cx);
    let feed = |event| assert!(script.feeds.borrow()[0].event(event));
    feed(Event::Loaded {
        video: true,
        duration_ms: 20_000,
    });
    feed(Event::Failed(Failure::Helper));
    column::pump(cx);
    assert!(
        cx.debug_bounds("column-media-failed").is_some(),
        "the failure draws in the frame"
    );
    script.commands.borrow_mut().clear();
    cx.simulate_click(frame, Modifiers::none());
    column::pump(cx);
    assert!(
        script.commands.borrow().is_empty(),
        "the failed player sends nothing"
    );
    assert_eq!(script.feeds.borrow().len(), 1, "no second decoder starts");
}
