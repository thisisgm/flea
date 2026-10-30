use super::caption::{origins, PAINTED, TRACKING};
use super::hero::EmptyHero;
use super::hero_tests::{bounds, frame, open, read, run};
use crate::list::Typeface;
use flea::ui_logic_fb1::empty::ROTATE_MS;
use flea::ui_logic_fb1::{CLOSE_MS, OPEN_MS};
use gpui::{
    px, size, AppContext, FontWeight, Pixels, ShapedLine, SharedString, TestAppContext, TextRun,
    WindowHandle,
};

/// A reduced-motion hero drawn in a 908x619 window; returns the window's scale.
pub(super) fn drawn(cx: &mut TestAppContext) -> (WindowHandle<EmptyHero>, f32) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    frame(cx, handle);
    frame(cx, handle);
    let scale = handle
        .update(cx, |_, window, _| window.scale_factor())
        .unwrap();
    (handle, scale)
}

/// The phrase shaped by the window's own text system at the caption's face, bold 12px.
pub(super) fn shaped(
    cx: &mut TestAppContext,
    handle: WindowHandle<EmptyHero>,
    text: SharedString,
) -> ShapedLine {
    let typeface = Typeface::omarchy_default();
    let mut font = typeface.font.clone();
    font.weight = FontWeight::BOLD;
    let run = TextRun {
        len: text.len(),
        font,
        color: crate::palette::Theme::default().foreground,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    handle
        .update(cx, |_, window, _| {
            window
                .text_system()
                .shape_line(text, typeface.caption, &[run], None)
        })
        .unwrap()
}

/// What the caption's paint placed, glyph x from its left edge, over one frame.
pub(super) fn painted<V: 'static>(cx: &mut TestAppContext, handle: WindowHandle<V>) -> Vec<Pixels> {
    PAINTED.with(|painted| *painted.borrow_mut() = Some(Vec::new()));
    cx.update_window(handle.into(), |_, window, _| window.refresh())
        .unwrap();
    frame(cx, handle);
    PAINTED.with(|painted| painted.borrow_mut().take()).unwrap()
}

/// `.statecap` letter-spacing 1.2px: paint places every glyph 1.2px further than the last, and the box grows by 1.2px per glyph.
#[gpui::test]
fn the_drawn_caption_tracks_every_glyph_by_the_boards_1_2px(cx: &mut TestAppContext) {
    let (handle, scale) = drawn(cx);
    let phrase = read(cx, handle, |hero| hero.phrases()[0].clone());
    let line = shaped(cx, handle, phrase.clone());
    let plain = origins(&line, px(0.));
    assert_eq!(plain.len(), phrase.chars().count());
    let xs = painted(cx, handle);
    assert_eq!(xs.len(), plain.len(), "paint placed every glyph once");
    for i in 1..plain.len() {
        let advance = (xs[i] - xs[i - 1]) - (plain[i] - plain[i - 1]);
        assert!(
            (advance - px(TRACKING)).abs() < px(0.001),
            "painted glyph {i} advance grows {advance:?}"
        );
    }
    let caption = read(cx, handle, |hero| {
        hero.painted.get("caption").unwrap().bounds
    });
    let (grown, want) = (
        caption.size.width - line.width,
        px(TRACKING) * plain.len() as f32,
    );
    assert!(
        (grown - want).abs() <= px(0.5 / scale),
        "drawn caption grew {grown:?} over its shaped width, the board asks {want:?}"
    );
}

/// One caption line for every beat (CTL2 2026-10-30): the first phrase and each beat draw gap 9 and a 16px line, and the caption never moves.
#[gpui::test]
fn every_beat_draws_one_caption_line_and_the_caption_never_moves(cx: &mut TestAppContext) {
    let t0 = cx.executor().now();
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    run(cx, handle, 400);
    let first = bounds(cx, handle, &["fb1-hero-mark", "fb1-hero-caption"]);
    assert_eq!(first[1].origin.y - first[0].bottom(), px(9.));
    assert_eq!(first[1].size.height, px(16.));
    assert_eq!(first[0].center().x, px(454.));
    while (cx
        .executor()
        .now()
        .saturating_duration_since(t0)
        .as_millis() as u64)
        <= 2 * ROTATE_MS + CLOSE_MS + OPEN_MS
    {
        frame(cx, handle);
        let parts = bounds(cx, handle, &["fb1-hero-mark", "fb1-hero-caption"]);
        assert_eq!(
            (parts[0].origin.y, parts[1].origin.y, parts[1].size.height),
            (first[0].origin.y, first[1].origin.y, first[1].size.height),
            "the hero moved"
        );
    }
    assert!(
        read(cx, handle, |hero| hero.rotation().beats()) >= 2,
        "two beats rotated"
    );
}
