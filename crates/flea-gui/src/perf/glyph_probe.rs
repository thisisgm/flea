//! Test-only observations of the pinned renderer's retained glyph path.
use gpui::{
    canvas, font, point, px, size, GlyphRun, Hsla, IntoElement, Styled, TestAppContext,
    TestDispatcher, TextRun, VisualTestContext, Window,
};
use std::{cell::Cell, rc::Rc};

#[derive(Clone, Copy, Default, Debug)]
pub struct Sample {
    pub allocations: u64,
    pub resolves: usize,
}

pub fn app() -> TestAppContext {
    let text = gpui_platform::current_platform(true).text_system();
    TestAppContext::build_with_text_system(TestDispatcher::new(0), None, text)
}

pub fn ink(cx: &mut VisualTestContext) -> Hsla {
    let ink = cx.update(|window, _| window.text_style().color);
    assert!(ink.a > 0., "the observed glyphs must use visible theme ink");
    ink
}

pub fn line(cx: &mut VisualTestContext, text: &str, font_size: f32) -> Rc<GlyphRun> {
    cx.update(|window, _| {
        let run = TextRun {
            len: text.len(),
            font: font("JetBrains Mono"),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let layout = window
            .text_system()
            .layout_line(text, px(font_size), &[run], None);
        assert!(layout.runs.iter().any(|run| !run.glyphs.is_empty()));
        Rc::new(GlyphRun::new(layout))
    })
}

pub fn paint(
    cx: &mut VisualTestContext,
    run: Rc<GlyphRun>,
    x: f32,
    y: f32,
    color: Hsla,
    allocations: fn() -> u64,
) -> Sample {
    observe(
        cx,
        move |window| {
            window
                .paint_glyph_run(point(px(x), px(y)), &run, color)
                .unwrap();
            run.resolves()
        },
        allocations,
    )
}

pub fn observe(
    cx: &mut VisualTestContext,
    operation: impl FnOnce(&mut Window) -> usize + 'static,
    allocations: fn() -> u64,
) -> Sample {
    let sample = Rc::new(Cell::new(Sample::default()));
    let observed = Rc::clone(&sample);
    cx.draw(
        point(px(0.), px(0.)),
        size(px(512.), px(256.)),
        move |_, _| {
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let before = allocations();
                    let resolves = operation(window);
                    observed.set(Sample {
                        allocations: allocations() - before,
                        resolves,
                    });
                },
            )
            .size_full()
            .into_any_element()
        },
    );
    sample.get()
}
