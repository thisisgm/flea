//! Text observations are attached where each Jobs label and run is built.
use super::palette::CardPalette;
use crate::g2::status_feedback::observe::Paint;
use flea::ui_logic_file_ops::Part;
use gpui::{div, prelude::*, Div, SharedString, StyleRefinement};

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

/// A failed job's reason wraps within the card, as the Delete sentence does (EY-350), so its end is never cut; a running file name stays one line.
pub(super) fn file(
    palette: &CardPalette,
    text: &str,
    paint: Option<&Paint>,
    wrap: bool,
) -> Option<Div> {
    if text.is_empty() {
        return None;
    }
    let element = if wrap {
        div()
            .text_size(gpui::px(flea::ui_logic_file_ops::card::JOBS_SMALL))
            .line_height(gpui::px(flea::ui_logic_file_ops::card::JOBS_SMALL_LINE))
            .text_color(palette.muted)
            .child(SharedString::from(text.to_owned()))
    } else {
        super::view::small(text.into(), palette.muted)
    };
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
