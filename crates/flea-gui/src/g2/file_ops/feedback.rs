//! Text observations are attached where each Jobs label and run is built.
use super::palette::CardPalette;
use crate::g2::status_feedback::observe::Paint;
use flea::ui_logic_file_ops::{
    card,
    jobs::{Block, State},
    Part,
};
use gpui::{div, prelude::*, px, Div, SharedString, StyleRefinement};

pub(super) struct Label<'a> {
    element: Div,
    paint: Option<&'a Paint>,
}

pub(super) fn label(paint: Option<&Paint>) -> Label<'_> {
    Label {
        element: div().relative(),
        paint,
    }
}

pub(super) fn file(palette: &CardPalette, block: &Block, paint: Option<&Paint>) -> Option<Div> {
    let text = &block.file;
    let id = block.id;
    if text.is_empty() {
        return None;
    }
    // A failure's reason is a sentence; only an in-flight file name stays on one line.
    let element = if block.state == State::Failed {
        div()
            .text_size(px(card::JOBS_SMALL))
            .line_height(px(card::JOBS_SMALL_LINE))
            .text_color(palette.muted)
            .whitespace_normal()
            .line_clamp(2)
            .child(text.clone())
    } else {
        super::view::small(text.clone(), palette.muted)
    }
    .debug_selector(move || format!("op1-job-{id}-source"));
    Some(match paint {
        Some(paint) => paint.watch(element.relative(), "source", text.to_owned().into()),
        None => element,
    })
}

impl Styled for Label<'_> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl Label<'_> {
    pub fn child(self, text: String) -> Div {
        let text: SharedString = text.into();
        let element = self.element.child(text.clone());
        match self.paint {
            Some(paint) => paint.watch(element, "headline", text),
            None => element,
        }
    }
}

pub(super) struct Runs {
    element: Div,
    text: SharedString,
}

impl Styled for Runs {
    fn style(&mut self) -> &mut StyleRefinement {
        self.element.style()
    }
}

impl Runs {
    pub fn observed(self, paint: Option<&Paint>) -> Div {
        match paint {
            Some(paint) => paint.watch(self.element.relative(), "byte-line", self.text),
            None => self.element,
        }
    }
}

pub(super) fn runs(palette: &CardPalette, parts: &[Part]) -> Runs {
    let mut element = div().flex().flex_row().whitespace_nowrap();
    let mut text = String::new();
    for part in parts {
        let ink = if part.figure {
            palette.foreground
        } else {
            palette.muted
        };
        element = element.child(div().text_color(ink).child(part.text.clone()));
        text.push_str(&part.text);
    }
    Runs {
        element,
        text: text.into(),
    }
}
