use super::hero::{EmptyHero, BOARDS};
use super::mark::HERO;
use super::state_tests::centred;
use crate::list::Typeface;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_fb1::empty::ROTATE_MS;
use gpui::{
    div, font, prelude::*, px, size, AppContext, Bounds, Context, Entity, FontWeight, Pixels,
    Render, TestAppContext, TextRun, VisualTestContext, Window, WindowHandle,
};
use std::time::Duration;

pub(super) const FRAME: u64 = 16;

pub(super) fn english() -> I18n {
    I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent/language".into(),
        catalogs: vec![crate::test_catalogs::english_only()],
    })
}

/// One platform frame: the clock moves, due timers run and the window draws what was dirtied; returns the frames asked for.
pub(super) fn frame<V: 'static>(cx: &mut TestAppContext, handle: WindowHandle<V>) -> usize {
    cx.executor().advance_clock(Duration::from_millis(FRAME));
    cx.run_until_parked();
    let asked = cx
        .update_window(handle.into(), |_, window, cx| {
            window.simulate_next_frame(cx)
        })
        .unwrap();
    cx.run_until_parked();
    asked
}

/// Frames asked for over `ms` of platform frames.
pub(super) fn run<V: 'static>(cx: &mut TestAppContext, handle: WindowHandle<V>, ms: u64) -> usize {
    (0..ms / FRAME).map(|_| frame(cx, handle)).sum()
}

pub(super) fn open(cx: &mut TestAppContext) -> WindowHandle<EmptyHero> {
    let catalog = english();
    let handle = cx.add_window(|window, cx| {
        window.set_rem_size(px(14.));
        EmptyHero::new(&catalog, cx)
    });
    handle
        .update(cx, |hero, _, cx| hero.show(true, cx))
        .unwrap();
    cx.run_until_parked();
    handle
}

pub(super) fn read<R>(
    cx: &mut TestAppContext,
    handle: WindowHandle<EmptyHero>,
    f: impl FnOnce(&EmptyHero) -> R,
) -> R {
    handle.update(cx, |hero, _, _| f(hero)).unwrap()
}

#[gpui::test]
fn the_entrance_and_caption_draw_on_frames_then_a_timer_waits_for_the_beat(
    cx: &mut TestAppContext,
) {
    let handle = open(cx);
    assert!(run(cx, handle, 2400) > 0, "the entrance asks for frames");
    assert!(read(cx, handle, |hero| !hero
        .rotation()
        .caption_animating()));
    let renders = read(cx, handle, |hero| hero.counters.renders.get());
    assert_eq!(
        run(cx, handle, 320),
        0,
        "between beats nothing asks for a frame"
    );
    assert_eq!(
        read(cx, handle, |hero| hero.counters.renders.get()),
        renders
    );
    assert!(read(cx, handle, |hero| hero.counters.timers.get()) >= 1);
    run(cx, handle, ROTATE_MS - 2400 - 320 + 2 * FRAME);
    assert_eq!(
        read(cx, handle, |hero| (
            hero.rotation().beats(),
            hero.rotation().caption_animating()
        )),
        (1, true)
    );
}

/// v0.3.12's Timer fires every 2800 ms from the show; each phrase swaps on the first frame at or after its fade out (140 ms).
#[gpui::test]
fn beats_fire_every_2800_ms_on_the_gpui_clock_and_swap_after_the_fade_out(cx: &mut TestAppContext) {
    let handle = open(cx);
    let phrase = |cx: &mut TestAppContext| {
        read(cx, handle, |hero| {
            (hero.rotation().beats(), hero.rotation().index())
        })
    };
    run(cx, handle, ROTATE_MS - FRAME);
    assert_eq!(phrase(cx), (0, 0), "no beat before 2800 ms");
    run(cx, handle, FRAME);
    assert_eq!(
        phrase(cx),
        (1, 0),
        "first beat at 2800 ms, fade out running"
    );
    run(cx, handle, 128);
    assert_eq!(
        phrase(cx),
        (1, 0),
        "phrase holds until its 140 ms fade out ends"
    );
    run(cx, handle, FRAME);
    assert_eq!(phrase(cx), (1, 1), "first swap on the frame at 2944 ms");
    run(cx, handle, 2640);
    assert_eq!(phrase(cx), (1, 1), "second beat not before 5600 ms");
    run(cx, handle, FRAME);
    assert_eq!(phrase(cx), (2, 1), "second beat at 5600 ms");
    run(cx, handle, 128);
    assert_eq!(phrase(cx), (2, 1), "second phrase holds until 5744 ms");
    run(cx, handle, FRAME);
    assert_eq!(phrase(cx), (2, 2), "second swap on the frame at 5744 ms");
}

#[gpui::test]
fn reduced_motion_draws_the_first_phrase_whole_and_never_wakes(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = open(cx);
    assert_eq!(run(cx, handle, 3 * ROTATE_MS), 0);
    assert_eq!(
        read(cx, handle, |hero| (
            hero.rotation().index(),
            hero.counters.timers.get(),
            hero.rotation().at_rest()
        )),
        (0, 0, true)
    );
}

#[gpui::test]
fn a_hide_drops_the_timer_and_the_next_show_starts_at_the_first_phrase(cx: &mut TestAppContext) {
    let handle = open(cx);
    run(cx, handle, ROTATE_MS + 400);
    handle.update(cx, |hero, _, cx| hero.hide(cx)).unwrap();
    let renders = read(cx, handle, |hero| hero.counters.renders.get());
    // The one frame the last moving render already queued runs; the hidden render queues none.
    assert!(run(cx, handle, 2 * ROTATE_MS) <= 1);
    assert_eq!(run(cx, handle, ROTATE_MS), 0);
    assert!(read(cx, handle, |hero| hero.counters.renders.get()) <= renders + 2);
    assert_eq!(read(cx, handle, |hero| hero.rotation().index()), 1);
    handle
        .update(cx, |hero, _, cx| hero.show(false, cx))
        .unwrap();
    run(cx, handle, ROTATE_MS - FRAME);
    assert_eq!(read(cx, handle, |hero| hero.rotation().index()), 0);
    run(cx, handle, 2400 + FRAME);
    assert_eq!(read(cx, handle, |hero| hero.rotation().index()), 1);
}

/// Wall time of each moving frame on the headless platform, printed as a range for the report and held under the 8.33 ms budget.
#[gpui::test]
fn a_moving_frame_stays_inside_the_frame_budget(cx: &mut TestAppContext) {
    let handle = open(cx);
    let mut spent = Vec::new();
    for _ in 0..(2400 / FRAME) {
        let before = read(cx, handle, |hero| hero.counters.renders.get());
        let start = std::time::Instant::now();
        frame(cx, handle);
        let ms = start.elapsed().as_secs_f64() * 1000.;
        if read(cx, handle, |hero| hero.counters.renders.get()) > before {
            spent.push(ms);
        }
    }
    assert!(spent.len() >= (flea::ui_logic_fb1::OPEN_MS / FRAME) as usize);
    spent.sort_by(f64::total_cmp);
    let (low, median, high) = (spent[0], spent[spent.len() / 2], spent[spent.len() - 1]);
    eprintln!(
        "FB1_HERO_FRAME_MS min={low:.3} median={median:.3} max={high:.3} n={}",
        spent.len()
    );
    assert!(median < 8.33);
}

/// The hero draws a 66px mark, and every beat in every view the beat boards' gap 9 and 16px caption line; phrases in capitals.
#[gpui::test]
fn the_hero_takes_the_boards_sizes_and_raises_its_phrases(cx: &mut TestAppContext) {
    assert_eq!((HERO, BOARDS.gap, BOARDS.caption_line), (66., 9., 16.));
    let handle = open(cx);
    let phrases = read(cx, handle, |hero| hero.phrases().to_vec());
    assert_eq!(
        (
            phrases[0].as_ref(),
            phrases[1].as_ref(),
            phrases[3].as_ref()
        ),
        ("NOTHING HERE YET", "A VERY TIDY FOLDER", "QUIET IN HERE")
    );
}

/// The laid-out bounds of each tagged part, read from the frame the window drew.
pub(super) fn bounds<V: 'static>(
    cx: &mut TestAppContext,
    handle: WindowHandle<V>,
    parts: &[&'static str],
) -> Vec<Bounds<Pixels>> {
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    parts
        .iter()
        .map(|part| {
            vcx.debug_bounds(part)
                .unwrap_or_else(|| panic!("{part} is not drawn"))
        })
        .collect()
}

/// empty-beat, empty-rest, grid-empty and the column boards: a 66px mark, 9px, a 16px caption line, centred in the pane.
#[gpui::test]
fn the_drawn_hero_has_the_boards_mark_gap_and_caption_line(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    frame(cx, handle);
    frame(cx, handle);
    let drawn = bounds(cx, handle, &["fb1-hero-mark", "fb1-hero-caption"]);
    let (mark, caption) = (drawn[0], drawn[1]);
    assert_eq!((mark.size.width, mark.size.height), (px(HERO), px(HERO)));
    assert_eq!(caption.origin.y - mark.bottom(), px(9.));
    assert_eq!(caption.size.height, px(16.));
    let scale = handle
        .update(cx, |_, window, _| window.scale_factor())
        .unwrap();
    assert_eq!(mark.center().x, px(454.));
    centred(caption.center().x, 454., scale);
    assert_eq!(
        mark.origin.y + (caption.bottom() - mark.origin.y) / 2.,
        px(619. / 2.)
    );
}

/// The real hero under a deliberately different inherited font and size; generic watch still observes that parent.
struct CaptionHost {
    hero: Entity<EmptyHero>,
    parent: super::observe::Paint,
}

impl Render for CaptionHost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.parent.watch(
            div()
                .relative()
                .size_full()
                .font_family("DejaVu Serif")
                .font_weight(FontWeight::NORMAL)
                .text_size(px(31.))
                .child(self.hero.clone()),
            "caption",
            self.hero.read(cx).phrases()[0].clone(),
        )
    }
}

#[gpui::test]
fn caption_observer_uses_the_painted_font_size_and_tracking(cx: &mut TestAppContext) {
    let root = super::test_root::root();
    let caseless = super::test_root::caseless(&root);
    assert_eq!(caseless.language(), "ja");
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = cx.add_window(|window, cx| {
        window.set_rem_size(px(14.));
        let hero = cx.new(|cx| EmptyHero::new(&english(), cx));
        hero.update(cx, |hero, cx| hero.show(false, cx));
        CaptionHost {
            hero,
            parent: Default::default(),
        }
    });
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    let changed = Typeface {
        font: font("DejaVu Sans"),
        ..Typeface::omarchy_default()
    };
    for (catalog, face, base, caption_size, tracking, phrase) in [
        (
            english(),
            Typeface::omarchy_default(),
            14.,
            12.,
            1.2,
            "NOTHING HERE YET",
        ),
        (
            english(),
            changed.clone(),
            16.,
            13.,
            1.2,
            "NOTHING HERE YET",
        ),
        (
            caseless,
            changed,
            16.,
            13.,
            0.,
            super::test_root::CASELESS_BEAT,
        ),
    ] {
        handle
            .update(cx, |host, window, cx| {
                cx.set_global(face.clone().at_base(base as u32));
                window.set_rem_size(px(base));
                host.hero.update(cx, |hero, cx| {
                    hero.translated(&catalog);
                    cx.notify();
                });
                cx.notify();
            })
            .unwrap();
        frame(cx, handle);
        frame(cx, handle);
        let (seen, parent, expected, scale) = handle
            .update(cx, |host, window, cx| {
                let mut font = face.font.clone();
                font.weight = FontWeight::BOLD;
                let run = TextRun {
                    len: phrase.len(),
                    font,
                    color: crate::palette::Theme::default().foreground,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let line =
                    window
                        .text_system()
                        .shape_line(phrase.into(), px(caption_size), &[run], None);
                let glyphs: usize = line.runs.iter().map(|run| run.glyphs.len()).sum();
                (
                    host.hero.read(cx).painted.get("caption").unwrap(),
                    host.parent.get("caption").unwrap(),
                    f32::from(line.width) + tracking * glyphs as f32,
                    window.scale_factor(),
                )
            })
            .unwrap();
        assert_eq!(
            parent.font_size, 31.,
            "the mismatched parent style reached paint"
        );
        assert!((parent.implicit_width - expected).abs() > 1.);
        assert_eq!(
            seen.font_size, caption_size,
            "caption font_size at base {base}"
        );
        assert_eq!(seen.text.as_ref(), phrase);
        assert!(
            (seen.implicit_width - expected).abs() < 0.001,
            "caption implicit_width {} differs from explicit shape {expected}",
            seen.implicit_width
        );
        let slot = bounds(cx, handle, &["fb1-hero-caption"])[0];
        assert_eq!(
            (seen.bounds.origin.y, seen.bounds.size.height),
            (slot.origin.y, slot.size.height)
        );
        assert!((seen.bounds.center().x - slot.center().x).abs() <= px(0.5 / scale));
        assert!((f32::from(seen.bounds.size.width) - expected).abs() <= 0.5 / scale);
    }
}
