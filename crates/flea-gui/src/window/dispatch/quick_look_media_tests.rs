//! Media kinds must mount a player through the real Shell rather than the unsupported recipe.
use super::preview_tests::{close, context, draw, file, opened};
use super::test_shell::{land, set_cursor};
use crate::window::u07_tests::read;
use flea::keys::Context;
use gpui::TestAppContext;

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
