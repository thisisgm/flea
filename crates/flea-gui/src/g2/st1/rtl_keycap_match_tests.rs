//! A matched label stays one text unit placed like its resting label in either window direction, and its highlight runs keep joining.
use super::{highlight, washed};
use crate::list::bench;
use gpui::{
    div, point, prelude::*, px, size, AvailableSpace, Bounds, Context, FontWeight, HighlightStyle,
    Hsla, Pixels, Render, ShapedLine, SharedString, VisualTestContext, Window,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

struct Host;

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

const WIDTH: f32 = 600.;
const TEXT_SIZE: f32 = 14.;
const CTRL_TAB: (&str, usize, usize) = ("ctrl-tab", 5, 8);
const SHIFT_TAB: (&str, usize, usize) = ("shift-tab", 6, 9);
const ARABIC: (&str, usize, usize) = ("الملفات", 2, 5);
const HEBREW: (&str, usize, usize) = ("קבצים", 1, 3);

fn context() -> VisualTestContext {
    let mut app = bench::real_cx();
    let handle = app.open_window(size(px(WIDTH), px(100.)), |_, _| Host);
    VisualTestContext::from_window(handle.into(), &app)
}

fn drawn(
    cx: &mut VisualTestContext,
    label: &str,
    run: Option<(usize, usize)>,
    lifted: bool,
    mirrored: bool,
) -> Vec<Bounds<Pixels>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = Rc::clone(&seen);
    cx.draw(
        point(px(0.), px(0.)),
        size(
            AvailableSpace::Definite(px(WIDTH)),
            AvailableSpace::MinContent,
        ),
        |window, _| {
            window.set_layout_mirrored(mirrored);
            washed(label, run, lifted, Hsla::default(), 0.)
                .text_size(px(TEXT_SIZE))
                .on_children_prepainted(move |bounds, _, _| *sink.borrow_mut() = bounds)
        },
    );
    seen.take()
}

// The label shaped with the runs washed() hands its StyledText, or as one plain run.
fn shaped(
    cx: &mut VisualTestContext,
    label: &str,
    run: Option<(usize, usize)>,
    lifted: bool,
) -> ShapedLine {
    cx.update(|window, _| {
        let plain = window.text_style();
        let mut runs = Vec::new();
        let mut covered = 0;
        if let Some((start, end)) = run {
            let (range, style) = highlight(label, start, end, lifted, Hsla::default());
            runs.push(plain.to_run(range.start));
            runs.push(plain.clone().highlight(style).to_run(range.len()));
            covered = range.end;
        }
        runs.push(plain.to_run(label.len() - covered));
        runs.retain(|run| run.len > 0);
        window.text_system().shape_line(
            SharedString::from(label.to_owned()),
            px(TEXT_SIZE),
            &runs,
            None,
        )
    })
}

fn glyphs(line: &ShapedLine) -> HashMap<usize, u32> {
    line.runs
        .iter()
        .flat_map(|run| &run.glyphs)
        .map(|glyph| (glyph.index, glyph.id.0))
        .collect()
}

fn byte(label: &str, character: usize) -> usize {
    label
        .char_indices()
        .nth(character)
        .map_or(label.len(), |(at, _)| at)
}

#[test]
fn sol_rtl_keycap_match_label_moves_as_one_unit_like_the_resting_label() {
    let mut cx = context();
    for (label, start, end) in [CTRL_TAB, SHIFT_TAB, ARABIC, HEBREW] {
        for mirrored in [false, true] {
            let resting = drawn(&mut cx, label, None, false, mirrored);
            for lifted in [false, true] {
                let matched = drawn(&mut cx, label, Some((start, end - start)), lifted, mirrored);
                assert_eq!(
                    matched.len(),
                    1,
                    "{label}: one text unit; mirrored={mirrored}, lifted={lifted}"
                );
                assert_eq!(
                    matched, resting,
                    "{label}: placed as the resting label; mirrored={mirrored}, lifted={lifted}"
                );
            }
        }
    }
}

#[test]
fn sol_rtl_keycap_match_highlight_keeps_the_recipe_and_character_range() {
    let wash = crate::palette::Theme::default().accent.opacity(0.14);
    let bold = HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        ..HighlightStyle::default()
    };
    let washed = HighlightStyle {
        background_color: Some(wash),
        ..HighlightStyle::default()
    };
    assert_eq!(highlight("ctrl-tab", 5, 8, false, wash), (5..8, washed));
    assert_eq!(highlight("ctrl-tab", 5, 8, true, wash), (5..8, bold));
    let (label, start, end) = ARABIC;
    assert_eq!(
        highlight(label, start, end, false, wash).0,
        byte(label, start)..byte(label, end)
    );
}

#[test]
fn sol_rtl_keycap_match_highlight_runs_keep_arabic_and_hebrew_joining() {
    let mut cx = context();
    for (label, start, end) in [ARABIC, HEBREW] {
        let plain = glyphs(&shaped(&mut cx, label, None, false));
        for lifted in [false, true] {
            let matched = glyphs(&shaped(&mut cx, label, Some((start, end)), lifted));
            for (index, id) in &plain {
                if *index < byte(label, start) || *index >= byte(label, end) {
                    assert_eq!(
                        matched.get(index),
                        Some(id),
                        "{label}: joined form kept at byte {index}; lifted={lifted}"
                    );
                }
            }
        }
    }
}
