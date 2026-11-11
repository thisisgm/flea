//! The Quick Look video decoder sizes to the card's device pixels.
use super::*;

#[gpui::test]
fn preview_quality_look_video_decoder_takes_the_card_device_pixels(cx: &mut TestAppContext) {
    let (shell, cx, _requests, script) = mounted(cx);
    cx.simulate_scale_factor_change(1.5);
    open_kind(&shell, cx, "clip.mp4");
    assert!(script.feeds.borrow()[0].event(Event::Loaded {
        video: true,
        duration_ms: 20_000,
    }));
    draw(cx);
    let View::Media(media) = mounted_view(&shell, cx) else {
        panic!("a video is mounted")
    };
    let decoder = media.player.read_with(cx, |player, _| {
        player
            .subject
            .as_ref()
            .map(|subject| (subject.box_w, subject.box_h))
    });
    let card = cx.update(|window, cx| {
        let viewport = window.viewport_size();
        shell
            .read(cx)
            .quick_look
            .as_ref()
            .unwrap()
            .view
            .read(cx)
            .card(viewport)
    });
    let want = crate::preview_quality::physical(
        f64::from(card.size.width),
        f64::from(card.size.height) - f64::from(crate::g2::media_preview::ink::size::STRIP),
        1.5,
    );
    assert_eq!(
        decoder,
        Some(want),
        "the decoder box is the card above its strip, in device pixels"
    );
    close(&shell, cx);
}
