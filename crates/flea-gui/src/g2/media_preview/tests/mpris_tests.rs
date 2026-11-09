//! The MPRIS session: its helper starts on the first play, speaks only on changes and obeys its remote.
use super::fake::{Root, Script};
use super::player_tests::{player, send, subject};
use flea::ui_logic_media_preview::frame::{Command, Event};
use flea::ui_logic_media_preview::session::Remote;
use gpui::TestAppContext;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

/// An MPRIS stand-in: it notes its start, asks for Pause once, then appends every state line it is given beside itself.
fn stub(root: &Path) -> (PathBuf, PathBuf) {
    let (program, lines) = (root.join("flea-mpris"), root.join("lines"));
    let starts = root.join("starts");
    let body = format!("#!/bin/sh\nprintf 'start\\n' >> '{}'\nprintf 'Pause\\n'\nwhile IFS= read -r line; do printf '%s\\n' \"$line\" >> '{}'; done\n", starts.display(), lines.display());
    std::fs::write(&program, body).expect("stub");
    std::fs::set_permissions(
        &program,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .expect("chmod");
    (program, lines)
}

/// Waits on real threads, the MPRIS helper's, settling the app between looks.
fn eventually(cx: &mut TestAppContext, mut done: impl FnMut(&mut TestAppContext) -> bool) -> bool {
    for _ in 0..400 {
        cx.run_until_parked();
        if done(cx) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

// tests/mediasession.qml: no helper before the first play, one line per state or seek, none per clock tick, and its commands obeyed.
#[gpui::test]
fn pv3_the_mpris_helper_starts_on_the_first_play_and_speaks_only_on_changes(
    cx: &mut TestAppContext,
) {
    // The helper is a real process read on a real thread, so its replies wake the app from outside the test scheduler.
    cx.executor().allow_parking();
    let root = Root::new();
    let (program, lines) = stub(&root.0);
    let script = Rc::new(Script::default());
    let (player, _) = player(cx, &script, Some(program));
    player.update(cx, |player, cx| player.open(subject("clip.mp4", true), cx));
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 2_000,
        },
    );
    std::thread::sleep(Duration::from_millis(50));
    assert!(!lines.exists(), "nothing before the first play");
    send(cx, &script, Event::Playing);
    let count =
        |lines: &Path| std::fs::read_to_string(lines).map_or(0, |text| text.lines().count());
    assert!(
        eventually(cx, |_| count(&lines) == 1),
        "the first line arrives once the helper is up"
    );
    let first = std::fs::read_to_string(&lines).unwrap();
    assert!(
        first.starts_with(
            r#"{"status":"Playing","title":"clip.mp4","url":"file:///m/clip.mp4","length":2000"#
        ),
        "{first}"
    );
    assert!(
        eventually(cx, |_| script.commands.borrow().contains(&Command::Pause)),
        "the helper's Pause reached the decoder"
    );
    for ms in [100, 200, 300] {
        send(cx, &script, Event::Position(ms));
    }
    send(cx, &script, Event::Seeked(1_500));
    assert!(eventually(cx, |_| count(&lines) == 2));
    let last = std::fs::read_to_string(&lines).unwrap();
    assert!(
        last.lines()
            .last()
            .unwrap()
            .ends_with(r#""position":1500,"seeked":true}"#),
        "{last}"
    );
    player.update(cx, |player, cx| player.close(cx));
}

// tests/mediasession.qml: a pause is one more line, the helper's commands are obeyed, and every player starts its own helper.
#[gpui::test]
fn pv3_the_session_obeys_its_remote_and_each_player_has_its_own_helper(cx: &mut TestAppContext) {
    // The helper is a real process read on a real thread, so its replies wake the app from outside the test scheduler.
    cx.executor().allow_parking();
    let root = Root::new();
    let (program, lines) = stub(&root.0);
    let count = |path: &Path| std::fs::read_to_string(path).map_or(0, |text| text.lines().count());
    let script = Rc::new(Script::default());
    let (video, _) = player(cx, &script, Some(program.clone()));
    video.update(cx, |player, cx| player.open(subject("clip.mp4", true), cx));
    send(
        cx,
        &script,
        Event::Loaded {
            video: true,
            duration_ms: 2_000,
        },
    );
    send(cx, &script, Event::Playing);
    assert!(eventually(cx, |_| count(&lines) == 1));
    assert!(
        eventually(cx, |_| script.commands.borrow().contains(&Command::Pause)),
        "the helper's Pause is in"
    );
    send(cx, &script, Event::Paused);
    assert!(
        eventually(cx, |_| count(&lines) == 2),
        "a pause is one more line"
    );
    let text = std::fs::read_to_string(&lines).unwrap();
    assert!(
        text.lines()
            .last()
            .unwrap()
            .starts_with(r#"{"status":"Paused""#),
        "{text}"
    );
    script.take_commands();
    let remote = |remote: Remote, cx: &mut TestAppContext| {
        video.update(cx, |player, cx| player.remote(remote, cx))
    };
    remote(Remote::SetPosition(1_200), cx);
    remote(Remote::Seek(300), cx);
    remote(Remote::PlayPause, cx);
    remote(Remote::Stop, cx);
    assert_eq!(
        script.take_commands(),
        [
            Command::Seek(1_200),
            Command::Seek(1_500),
            Command::Play,
            Command::Stop
        ]
    );
    let sound = Rc::new(Script::default());
    let (audio, _) = player(cx, &sound, Some(program));
    audio.update(cx, |player, cx| player.open(subject("tone.ogg", false), cx));
    send(
        cx,
        &sound,
        Event::Loaded {
            video: false,
            duration_ms: 2_000,
        },
    );
    send(cx, &sound, Event::Playing);
    assert!(
        eventually(cx, |_| count(&root.0.join("starts")) == 2),
        "the audio player started a second helper"
    );
    assert!(eventually(cx, |_| std::fs::read_to_string(&lines)
        .unwrap_or_default()
        .contains(r#"{"status":"Playing","title":"tone.ogg""#)));
    video.update(cx, |player, cx| player.close(cx));
    audio.update(cx, |player, cx| player.close(cx));
}
