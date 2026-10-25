//! Quick Look's IPC clock follows its own player and returns the column clock's absent sentinel otherwise.
use super::preview_tests::{close, draw};
use super::quick_look_body_tests::{mounted, open_kind};
use crate::window::u07_tests::read;
use flea::ui_logic_pv3::frame::Event;
use gpui::TestAppContext;

fn clock(cx: &mut TestAppContext, name: &str, video: bool) {
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, name);
    for event in [
        Event::Loaded {
            video,
            duration_ms: 20_000,
        },
        Event::Playing,
        Event::Position(1_000),
    ] {
        assert!(script.feeds.borrow()[0].event(event));
    }
    draw(cx);
    assert_eq!(read(&shell, cx, "previewPosition", &[]), "1000");
    assert!(script.feeds.borrow()[0].event(Event::Position(1_750)));
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewPosition", &[]),
        "1750",
        "IPC sees the advancing Quick Look player clock in milliseconds"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert_eq!(
        read(&shell, cx, "previewPosition", &[]),
        "-1",
        "a closed card has no clock"
    );
    open_kind(&shell, cx, "notes.txt");
    assert_eq!(read(&shell, cx, "previewOpen", &[]), "true");
    assert_eq!(read(&shell, cx, "previewKind", &[]), "text");
    assert_eq!(
        read(&shell, cx, "previewPosition", &[]),
        "-1",
        "a document has no media clock"
    );
    close(&shell, cx);
}

#[gpui::test]
fn audio_preview_position_reads_the_advancing_quick_look_clock(cx: &mut TestAppContext) {
    clock(cx, "song.ogg", false);
}

#[gpui::test]
fn video_preview_position_reads_the_advancing_quick_look_clock(cx: &mut TestAppContext) {
    clock(cx, "clip.mp4", true);
}
