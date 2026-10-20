use super::caption::{origins, PAINTED, TRACKING};
use super::hero::EmptyHero;
use super::hero_tests::{bounds, frame, open, read, run};
use super::state_tests::{centred, snapped};
use crate::list::Typeface;
use flea::ui_logic_fb1::empty::ROTATE_MS;
use flea::ui_logic_fb1::{CLOSE_MS, OPEN_MS};
use gpui::{
    px, size, AppContext, FontWeight, Pixels, ShapedLine, SharedString, TestAppContext, TextRun,
    WindowHandle,
};

/// A reduced-motion hero drawn in a 908x619 window, in the list view or not; returns the window's scale.
pub(super) fn drawn(cx: &mut TestAppContext, in_list: bool) -> (WindowHandle<EmptyHero>, f32) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = open(cx);
    handle
        .update(cx, |hero, _, cx| hero.set_in_list(in_list, cx))
        .unwrap();
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
    let (handle, scale) = drawn(cx, false);
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
    let caption = bounds(cx, handle, &["fb1-hero-caption"])[0];
    let (grown, want) = (
        caption.size.width - line.width,
        px(TRACKING) * plain.len() as f32,
    );
    assert!(
        (grown - want).abs() <= px(0.5 / scale),
        "drawn caption grew {grown:?} over its shaped width, the board asks {want:?}"
    );
}

/// RULING (controller, 02:10): empty.html, the list's first phrase, draws gap 10 and its 16.8px caption box.
#[gpui::test]
fn the_list_hero_before_its_first_beat_draws_empty_html_gap_10(cx: &mut TestAppContext) {
    let (handle, scale) = drawn(cx, true);
    let drawn = bounds(cx, handle, &["fb1-hero-mark", "fb1-hero-caption"]);
    let (mark, caption) = (drawn[0], drawn[1]);
    assert_eq!(caption.origin.y - mark.bottom(), px(10.));
    assert_eq!(caption.size.height, snapped(16.8, scale));
    assert_eq!(mark.center().x, px(454.));
    centred(caption.center().x, 454., scale);
}

/// empty-beat and empty-rest are the same list hero after a beat, so they draw their own gap 9 and 16px line.
#[gpui::test]
fn the_list_hero_after_a_beat_draws_the_beat_boards_gap_9(cx: &mut TestAppContext) {
    let handle = open(cx);
    handle
        .update(cx, |hero, _, cx| hero.set_in_list(true, cx))
        .unwrap();
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    run(cx, handle, ROTATE_MS + 2400);
    assert_eq!(read(cx, handle, |hero| hero.rotation().beats()), 1);
    let drawn = bounds(cx, handle, &["fb1-hero-mark", "fb1-hero-caption"]);
    assert_eq!(drawn[1].origin.y - drawn[0].bottom(), px(9.));
    assert_eq!(drawn[1].size.height, px(16.));
}

/// The list's switch to 9/16 happens only as the first beat's swap lands: caption out, mark undrawn, as the OEM phraseSwap swaps at opacity 0.
#[gpui::test]
fn the_list_heros_first_beat_never_moves_an_opaque_hero(cx: &mut TestAppContext) {
    let t0 = cx.executor().now();
    let handle = open(cx);
    handle
        .update(cx, |hero, _, cx| hero.set_in_list(true, cx))
        .unwrap();
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    run(cx, handle, 400);
    let mut last = None;
    let mut moves = 0;
    while cx
        .executor()
        .now()
        .saturating_duration_since(t0)
        .as_millis() as u64
        <= ROTATE_MS + CLOSE_MS + OPEN_MS
    {
        frame(cx, handle);
        let now = cx
            .executor()
            .now()
            .saturating_duration_since(t0)
            .as_millis() as u64;
        let (landed, drawn) = read(cx, handle, |hero| {
            (hero.rotation().landed(), hero.rotation().frame(now))
        });
        let parts = bounds(cx, handle, &["fb1-hero-mark", "fb1-hero-caption"]);
        let geometry = (parts[0].origin.y, parts[1].origin.y, parts[1].size.height);
        if last.is_some_and(|was| was != geometry) {
            moves += 1;
            assert!(landed, "{now}ms: the hero moved before its swap landed");
            assert!(
                drawn.caption < 0.25 && drawn.mark_drawn == 0. && drawn.phrase == 1,
                "{now}ms: the hero moved while drawn: {drawn:?}"
            );
        }
        last = Some(geometry);
    }
    assert_eq!(moves, 1, "one unseen switch to the beat boards' 9/16");
    let parts = bounds(cx, handle, &["fb1-hero-mark", "fb1-hero-caption"]);
    assert_eq!(parts[1].origin.y - parts[0].bottom(), px(9.));
}
