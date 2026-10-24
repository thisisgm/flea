use super::hero::{EmptyHero, BOARDS, LIST_FIRST};
use super::mark::HERO;
use super::state_tests::centred;
use crate::list::Typeface;
use flea::i18n::{I18n, Paths};
use flea::ui_logic_fb1::empty::{BEAT_LIMIT, ROTATE_MS};
use gpui::{
    div, font, prelude::*, px, size, AppContext, Bounds, Context, Entity, FontWeight, Pixels,
    Render, TestAppContext, TextRun, VisualTestContext, Window, WindowHandle,
};
use std::{path::Path, time::Duration};

pub(super) const FRAME: u64 = 16;

pub(super) fn english() -> I18n {
    I18n::load(Paths {
        omarchy: "/nonexistent".into(),
        language_file: "/nonexistent/language".into(),
        catalogs: vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n")],
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
fn the_entrance_and_mark_draw_on_frames_then_a_timer_waits_for_the_beat(cx: &mut TestAppContext) {
    let handle = open(cx);
    assert!(
        run(cx, handle, 2400) > 0,
        "the entrance and the mark's draw ask for frames"
    );
    assert!(read(cx, handle, |hero| !hero.rotation().animating()));
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
            hero.rotation().animating()
        )),
        (1, true)
    );
}

#[gpui::test]
fn after_three_beats_the_hero_rests_with_no_frame_and_no_timer(cx: &mut TestAppContext) {
    let handle = open(cx);
    run(cx, handle, 3 * ROTATE_MS + 2400);
    assert_eq!(
        read(cx, handle, |hero| (
            hero.rotation().beats(),
            hero.rotation().index(),
            hero.rotation().at_rest()
        )),
        (BEAT_LIMIT, 3, true)
    );
    let (renders, timers) = read(cx, handle, |hero| {
        (hero.counters.renders.get(), hero.counters.timers.get())
    });
    assert_eq!(run(cx, handle, 10_000), 0);
    assert_eq!(
        read(cx, handle, |hero| (
            hero.counters.renders.get(),
            hero.counters.timers.get()
        )),
        (renders, timers)
    );
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
fn a_hide_drops_the_timer_and_the_next_show_carries_the_phrase_on(cx: &mut TestAppContext) {
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
    run(cx, handle, 3 * ROTATE_MS + 2400);
    assert_eq!(read(cx, handle, |hero| hero.rotation().index()), 4);
}

/// Wall time of each moving frame on the headless platform, printed as a range for the report and held under the 8.33 ms budget.
#[gpui::test]
fn a_moving_frame_stays_inside_the_frame_budget(cx: &mut TestAppContext) {
    let handle = open(cx);
    let mut spent = Vec::new();
    for _ in 0..(2400 / FRAME) {
        let start = std::time::Instant::now();
        frame(cx, handle);
        spent.push(start.elapsed().as_secs_f64() * 1000.);
    }
    spent.sort_by(f64::total_cmp);
    let (low, median, high) = (spent[0], spent[spent.len() / 2], spent[spent.len() - 1]);
    eprintln!(
        "FB1_HERO_FRAME_MS min={low:.3} median={median:.3} max={high:.3} n={}",
        spent.len()
    );
    assert!(median < 8.33);
}

/// The hero boards draw a 48px mark; empty.html states gap 10 and a 16.8px caption box, the others 9 and 16; phrases in capitals.
#[gpui::test]
fn the_hero_takes_the_boards_sizes_and_raises_its_phrases(cx: &mut TestAppContext) {
    assert_eq!(
        (
            HERO,
            LIST_FIRST.gap,
            LIST_FIRST.caption_line,
            BOARDS.gap,
            BOARDS.caption_line
        ),
        (48., 10., 16.8, 9., 16.)
    );
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

/// empty-beat, empty-rest, grid-empty and the column boards: a 48px mark, 9px, a 16px caption line, centred in the pane.
#[gpui::test]
fn the_drawn_hero_has_the_boards_mark_gap_and_caption_line(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = open(cx);
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    frame(cx, handle);
    frame(cx, handle);
    let drawn = bounds(cx, handle, &["fb1-hero-mark", "fb1-hero-caption"]);
    let (mark, caption) = (drawn[0], drawn[1]);
    assert_eq!((mark.size.width, mark.size.height), (px(48.), px(48.)));
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
    std::fs::create_dir_all(root.join("omarchy/default/i18n")).unwrap();
    std::fs::write(
        root.join("omarchy/default/i18n/locales.json"),
        r#"{"en":{},"ja":{"name":"Japanese"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("language"), "ja").unwrap();
    let caseless = I18n::load(Paths {
        omarchy: root.join("omarchy"),
        language_file: root.join("language"),
        catalogs: vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("../../i18n")],
    });
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
        (caseless, changed, 16., 13., 0., "Nothing here yet"),
    ] {
        handle
            .update(cx, |host, window, cx| {
                cx.set_global(face.clone());
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
        let drawn = bounds(cx, handle, &["fb1-hero-caption"])[0];
        assert_eq!(seen.bounds, drawn);
        assert!((f32::from(drawn.size.width) - expected).abs() <= 0.5 / scale);
    }
}
