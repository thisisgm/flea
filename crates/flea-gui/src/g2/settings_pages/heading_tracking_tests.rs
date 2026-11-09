//! S10V-2: a spaced heading sets each letter at its shaped advance plus the spacing, as the boards' CSS letter-spacing does.
use super::spaced;
use gpui::{
    canvas, div, prelude::*, px, Bounds, Context, FontWeight, Pixels, TestAppContext, TextRun,
    Window,
};
use std::{cell::Cell, rc::Rc};

const HEADING: &str = "BASIC FILE ACTIONS";
// A size whose advances are fractional, where a box per letter rounded up to whole pixels drifts.
const SIZE: f32 = 12.7;
const SPACING: f32 = 12.7 * 0.14;

#[derive(Default)]
struct Seen {
    drawn: Cell<Option<Bounds<Pixels>>>,
    advances: Cell<f32>,
    rounded: Cell<f32>,
}

struct Heading(Rc<Seen>);

impl Render for Heading {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mut font = window.text_style().font();
        font.weight = FontWeight::BOLD;
        let (mut advances, mut rounded) = (0., 0.);
        for letter in HEADING.chars() {
            let letter = letter.to_string();
            let run = TextRun {
                len: letter.len(),
                font: font.clone(),
                color: Default::default(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let advance = f32::from(
                window
                    .text_system()
                    .layout_line(&letter, px(SIZE), &[run], None)
                    .width,
            );
            advances += advance;
            rounded += advance.ceil();
        }
        self.0.advances.set(advances);
        self.0.rounded.set(rounded);
        let seen = Rc::clone(&self.0);
        div()
            .flex()
            .text_size(px(SIZE))
            .font_weight(FontWeight::BOLD)
            .child(
                div().relative().child(spaced(HEADING, SPACING)).child(
                    canvas(
                        move |bounds, _, _| seen.drawn.set(Some(bounds)),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .left_0()
                    .top_0()
                    .size_full(),
                ),
            )
    }
}

#[gpui::test]
fn s10v2_a_spaced_heading_keeps_the_fonts_own_pitch(cx: &mut TestAppContext) {
    let seen = Rc::new(Seen::default());
    let (_, cx) = cx.add_window_view(|_, _| Heading(Rc::clone(&seen)));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let drawn = f32::from(
        seen.drawn
            .get()
            .expect("the heading was laid out")
            .size
            .width,
    );
    let gaps = HEADING.chars().count() as f32 * SPACING;
    let want = seen.advances.get() + gaps;
    assert!(
        (drawn - want).abs() < 0.5,
        "drawn {drawn}px, the letters' advances plus the spacing are {want}px"
    );
    let rounded = seen.rounded.get() + gaps;
    if rounded - want > 1. {
        assert!(
            (drawn - rounded).abs() > 1.,
            "letters boxed at whole pixels draw {rounded}px, wider than the board"
        );
    }
}
