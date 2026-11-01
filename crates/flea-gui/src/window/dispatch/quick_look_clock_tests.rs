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

#[gpui::test]
fn gh364_down_starts_a_fresh_autoplay_video_and_accepts_its_picture_and_clock(
    cx: &mut TestAppContext,
) {
    use super::{
        preview_tests::{body, file},
        test_shell::{land, set_cursor},
    };
    use crate::g2::pv1::{doc::View, Body};
    use flea::ui_logic_pv3::player::Status;
    use gpui::RenderImage;
    use std::sync::Arc;

    let (shell, cx, _requests, script) = mounted(cx);
    cx.update(|window, cx| {
        shell.update(cx, |shell, cx| {
            assert!(shell
                .step(0, crate::window::navigate::Step::Refresh(None), window, cx)
                .unwrap());
        });
    });
    land(
        &shell,
        cx,
        "/files",
        vec![file("a.mp4", "video-mp4"), file("b.mp4", "video-mp4")],
        2,
    );
    set_cursor(&shell, cx, 0);
    cx.simulate_keystrokes("space");
    draw(cx);

    let mut previous = None;
    for (index, name) in ["a.mp4", "b.mp4", "a.mp4", "b.mp4"].into_iter().enumerate() {
        if index > 0 {
            cx.simulate_keystrokes(if index % 2 == 1 { "down" } else { "up" });
            draw(cx);
        }
        let Some(Body::Doc(doc)) = body(&shell, cx) else {
            panic!("video document for {name} at step {index}")
        };
        assert_eq!(doc.path, format!("/files/{name}"));
        let View::Media(media) = doc.view else {
            panic!("video player")
        };
        let id = media.player.entity_id();
        assert_ne!(
            previous,
            Some(id),
            "following a clip creates its own player"
        );
        previous = Some(id);
        media.player.read_with(cx, |player, _| {
            let subject = player.subject.as_ref().unwrap();
            assert_eq!(subject.path, std::path::Path::new("/files").join(name));
            assert!(subject.autoplay && subject.video);
            assert_eq!(player.status(), Status::Loading);
        });
        assert_eq!(script.feeds.borrow().len(), index + 1);
        let feed = script.feeds.borrow()[index].clone();
        assert!(feed.event(Event::Loaded {
            video: true,
            duration_ms: 20_000
        }));
        assert!(feed.event(Event::Playing));
        assert!(feed.event(Event::Position(1_000 + index as i64)));
        let buffer = image::RgbaImage::from_raw(2, 2, vec![index as u8; 16]).unwrap();
        let picture = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
        let picture_id = picture.id;
        feed.frame(picture);
        draw(cx);
        assert_eq!(
            read(&shell, cx, "previewPosition", &[]),
            (1_000 + index).to_string()
        );
        media.player.read_with(cx, |player, _| {
            assert_eq!(player.status(), Status::Playing);
            assert_eq!(
                player.frame.as_ref().map(|frame| frame.id),
                Some(picture_id)
            );
        });
    }
    close(&shell, cx);
}

// EY-435: an open Quick Look video reads previewMediaLoaded true, and previewPosition proves a seek key moved the clock.
#[gpui::test]
fn ey435_quick_look_media_loaded_and_seek_keys_move_the_position(cx: &mut TestAppContext) {
    use flea::ui_logic_pv1::keys::SEEK_MS;
    let (shell, cx, _requests, script) = mounted(cx);
    open_kind(&shell, cx, "clip.mp4");
    for event in [
        Event::Loaded {
            video: true,
            duration_ms: 20_000,
        },
        Event::Playing,
        Event::Position(1_000),
    ] {
        assert!(script.feeds.borrow()[0].event(event));
    }
    draw(cx);
    assert_eq!(read(&shell, cx, "previewMediaLoaded", &[]), "true");
    let before: i64 = read(&shell, cx, "previewPosition", &[]).parse().unwrap();
    cx.simulate_keystrokes("right");
    draw(cx);
    let forward: i64 = read(&shell, cx, "previewPosition", &[]).parse().unwrap();
    assert_eq!(forward, before + SEEK_MS, "seekForward");
    cx.simulate_keystrokes("left");
    draw(cx);
    let back: i64 = read(&shell, cx, "previewPosition", &[]).parse().unwrap();
    assert_eq!(back, before, "seekBack");
    open_kind(&shell, cx, "notes.txt");
    assert_eq!(read(&shell, cx, "previewMediaLoaded", &[]), "false");
    close(&shell, cx);
}
