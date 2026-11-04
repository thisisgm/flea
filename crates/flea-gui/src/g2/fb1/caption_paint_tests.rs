use super::caption::{origins, tracking_for, Tracked, TRACKING};
use super::caption_tests::{painted, shaped};
use super::hero::EmptyHero;
use super::hero_tests::frame;
use super::test_root::{caseless, fallback, root};
use crate::list::{bench::allocations, Typeface};
use gpui::{
    div, point, prelude::*, px, size, App, Bounds, Context, Element, ElementId, FontWeight,
    GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Render, ShapedLine,
    TestAppContext, Window,
};
use std::cell::Cell;

thread_local! {
    static SPENT: Cell<u64> = const { Cell::new(0) };
}

/// The caption with allocations counted only while it paints; the control places the same glyphs inline.
struct Counted(Tracked, bool);

impl IntoElement for Counted {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Counted {
    type RequestLayoutState = ShapedLine;
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ShapedLine) {
        self.0.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        shaped: &mut ShapedLine,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.0.prepaint(id, inspector, bounds, shaped, window, cx)
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        shaped: &mut ShapedLine,
        state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let before = allocations();
        if self.1 {
            let tracked = &self.0;
            for run in shaped.runs.iter() {
                for glyph in run.glyphs.iter() {
                    let origin = point(bounds.origin.x + glyph.position.x, bounds.origin.y);
                    let _ = window.paint_glyph(
                        origin,
                        run.font_id,
                        glyph.id,
                        shaped.font_size,
                        tracked.color,
                    );
                }
            }
        } else {
            self.0
                .paint(id, inspector, bounds, shaped, state, window, cx);
        }
        let spent = allocations() - before;
        SPENT.with(|total| total.set(total.get() + spent));
    }
}

/// A view that draws the first phrase on every frame, as the hero does while it moves.
struct Moving(bool);

impl Render for Moving {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let typeface = Typeface::omarchy_default();
        let mut font = typeface.font.clone();
        font.weight = FontWeight::BOLD;
        let tracked = Tracked {
            text: "NOTHING HERE YET".into(),
            font,
            size: typeface.caption,
            line: px(16.),
            tracking: px(TRACKING),
            color: crate::palette::Theme::default().foreground,
        };
        div().size_full().child(Counted(tracked, self.0))
    }
}

/// No allocation in paint: over 60 steady frames the caption's paint allocates nothing, as the inline control.
#[gpui::test]
fn the_tracked_caption_paints_without_allocating(cx: &mut TestAppContext) {
    let mut count = |inline: bool| {
        let handle = cx.add_window(|_, _| Moving(inline));
        cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
        for _ in 0..5 {
            frame(cx, handle);
        }
        SPENT.with(|total| total.set(0));
        for _ in 0..60 {
            frame(cx, handle);
        }
        SPENT.with(Cell::get)
    };
    let (caption, control) = (count(false), count(true));
    assert_eq!((caption, control), (0, 0));
}

/// The boards' caseless scripts draw the caption untracked and in its own case; region subtags count, Latin scripts track.
#[gpui::test]
fn a_caseless_language_draws_the_caption_untracked(cx: &mut TestAppContext) {
    for (code, tracking) in [
        ("ja", 0.),
        ("zh-TW", 0.),
        ("ko", 0.),
        ("ar", 0.),
        ("fa", 0.),
        ("he", 0.),
        ("ur", 0.),
        ("th", 0.),
        ("en", TRACKING),
        ("de", TRACKING),
        ("pt-BR", TRACKING),
    ] {
        assert_eq!(tracking_for(code), tracking, "{code}");
    }
    let root = root();
    let catalog = caseless(&root);
    assert_eq!(catalog.language(), "ja");
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = cx.add_window(|window, cx| {
        window.set_rem_size(px(14.));
        EmptyHero::new(&catalog, cx)
    });
    handle
        .update(cx, |hero, _, cx| hero.show(true, cx))
        .unwrap();
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    frame(cx, handle);
    let phrase = handle
        .update(cx, |hero, _, _| hero.phrases()[0].clone())
        .unwrap();
    assert_eq!(phrase.as_ref(), super::test_root::CASELESS_BEAT);
    let line = shaped(cx, handle, phrase);
    assert_eq!(painted(cx, handle), origins(&line, px(0.)));
    let caption = handle
        .update(cx, |hero, _, _| hero.painted.get("caption").unwrap().bounds)
        .unwrap();
    assert!((caption.size.width - line.width).abs() < px(0.5));
}

/// EY-544: Devanagari, Bengali, Tamil and Sinhala are caseless too, so a Hindi hero paints untracked and never cuts a cluster apart.
#[gpui::test]
fn an_indic_language_draws_the_caption_untracked(cx: &mut TestAppContext) {
    for code in ["hi", "bn", "ta", "si", "hi-IN"] {
        assert_eq!(tracking_for(code), 0., "{code}");
    }
    let root = root();
    std::fs::create_dir_all(root.join("catalog/en")).unwrap();
    std::fs::write(
        root.join("catalog/en/flea.json"),
        r#"{"pane.empty.initial":"Nothing here yet"}"#,
    )
    .unwrap();
    let catalog = fallback(&root, "hi");
    assert_eq!(catalog.language(), "hi");
    assert_eq!(catalog.tr("pane.empty.initial"), "Nothing here yet");
    cx.update(|cx| cx.set_reduce_motion(true));
    let handle = cx.add_window(|window, cx| {
        window.set_rem_size(px(14.));
        EmptyHero::new(&catalog, cx)
    });
    handle
        .update(cx, |hero, _, cx| hero.show(true, cx))
        .unwrap();
    cx.simulate_window_resize(handle.into(), size(px(908.), px(619.)));
    frame(cx, handle);
    let phrase = handle
        .update(cx, |hero, _, _| hero.phrases()[0].clone())
        .unwrap();
    assert_eq!(phrase.as_ref(), "Nothing here yet");
    let line = shaped(cx, handle, phrase);
    assert_eq!(painted(cx, handle), origins(&line, px(0.)));
}
