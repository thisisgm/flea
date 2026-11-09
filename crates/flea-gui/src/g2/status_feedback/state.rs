//! The one thing a pane shows instead of rows when a listing failed: the mark, one sentence, and the detail the system can prove.
use super::{
    bump,
    mark::{Glyph, StateMark},
    Counters, Ink,
};
use crate::list::{Palette, Typeface};
use flea::i18n::I18n;
use flea::ui_logic_status_feedback::errors::{listing_state, locked_line, sentence, ListingState};
use gpui::{div, prelude::*, px, Context, IntoElement, Render, SharedString, Window};
use std::rc::Rc;

/// The state's geometry on one surface, in the sizes its board states.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub top: f32,
    pub gap: f32,
    pub under_mark: f32,
    pub sentence_line: f32,
    pub detail_line: f32,
    pub detail_drop: f32,
}

/// The list boards, error and locked: a 10px gap, both lines at 1.2, the detail 2px further down.
pub const LIST: Metrics = Metrics {
    top: 0.,
    gap: 10.,
    under_mark: 0.,
    sentence_line: 14. * 1.2,
    detail_line: 12. * 1.2,
    detail_drop: 2.,
};

/// The grid-locked board: 3px down, a 9px gap with 1px more under the mark, the sentence at 1.32 and the detail on a 22px line.
pub const GRID: Metrics = Metrics {
    top: 3.,
    gap: 9.,
    under_mark: 1.,
    sentence_line: 14. * 1.32,
    detail_line: 22.,
    detail_drop: 0.,
};

impl Metrics {
    fn at(self, face: &Typeface) -> Self {
        let body = f32::from(face.body);
        let caption = f32::from(face.caption);
        Self {
            top: (self.top * body / 14.).round(),
            gap: (8. * body / 12.).round() + self.gap - GRID.gap,
            under_mark: self.under_mark,
            sentence_line: self.sentence_line * body / 14.,
            detail_line: self.detail_line * caption / 12.,
            detail_drop: self.detail_drop,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Locked,
    Error,
    /// A listing past its wait, drawn as the Error state.
    Waiting,
}

/// A failed listing as the pane draws it, translated once when it lands or the language changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
    pub kind: Kind,
    pub sentence: SharedString,
    pub detail: SharedString,
}

impl Facts {
    /// The backend's failure for the folder on screen; the breadcrumb names it, so the sentence does not.
    pub fn failed(place: &str, message: &str, mode: u32, catalog: &I18n) -> Self {
        let kind = match listing_state(place, message) {
            ListingState::Locked => Kind::Locked,
            ListingState::Error => Kind::Error,
        };
        let detail = if kind == Kind::Locked {
            locked_line(mode, catalog)
        } else {
            String::new()
        };
        Self {
            kind,
            sentence: sentence(place, message, "").render(catalog).into(),
            detail: detail.into(),
        }
    }

    pub fn waiting(catalog: &I18n) -> Self {
        Self {
            kind: Kind::Waiting,
            sentence: catalog.tr("window.waiting").into(),
            detail: SharedString::default(),
        }
    }

    pub fn glyph(&self) -> Glyph {
        if self.kind == Kind::Locked {
            Glyph::Lock
        } else {
            Glyph::Alert
        }
    }

    /// StateMessage.qml: only a failure takes the urgent role, and only on the mark; the sentence stays readable.
    pub fn mark_ink(&self, ink: Ink) -> gpui::Hsla {
        ink.error
    }
}

pub struct StateMessage {
    facts: Option<Facts>,
    metrics: Metrics,
    typeface: Typeface,
    pub counters: Rc<Counters>,
}

impl StateMessage {
    pub fn set_typeface(&mut self, typeface: Typeface, cx: &mut Context<Self>) {
        if self.typeface != typeface {
            self.typeface = typeface;
            cx.notify();
        }
    }
    pub fn new() -> Self {
        Self {
            facts: None,
            metrics: LIST,
            typeface: Typeface::omarchy_default(),
            counters: Rc::default(),
        }
    }

    pub fn facts(&self) -> Option<&Facts> {
        self.facts.as_ref()
    }

    /// The surface the pane draws in picks its board's geometry.
    pub fn set_metrics(&mut self, metrics: Metrics, cx: &mut Context<Self>) {
        if self.metrics != metrics {
            self.metrics = metrics;
            cx.notify();
        }
    }

    pub fn metrics(&self) -> Metrics {
        self.metrics
    }

    /// The pane's shared state surface clears the ancestor tracks before it is centred.
    pub fn natural_width(&self, window: &Window) -> f32 {
        let Some(facts) = &self.facts else { return 0. };
        [
            (facts.sentence.clone(), self.typeface.body),
            (facts.detail.clone(), self.typeface.caption),
        ]
        .into_iter()
        .map(|(text, size)| {
            super::caption::measure(text, self.typeface.font.clone(), size, px(0.), window)
        })
        .fold(super::mark::STATE, f32::max)
    }

    /// Shows a failure, or clears the surface with None; an unchanged value repaints nothing.
    pub fn set(&mut self, facts: Option<Facts>, cx: &mut Context<Self>) {
        if self.facts != facts {
            self.facts = facts;
            cx.notify();
        }
    }
}

impl Default for StateMessage {
    fn default() -> Self {
        Self::new()
    }
}

impl Render for StateMessage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        bump(&self.counters.renders);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let ink = Ink::interim(&Palette::inherited(&window.text_style(), theme));
        self.typeface = Typeface::for_window(window, cx);
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center();
        let Some(facts) = &self.facts else {
            return root;
        };
        let metrics = self.metrics.at(&self.typeface);
        let line = |part: &'static str, text: SharedString, size, height: f32, color| {
            div()
                .debug_selector(move || part.into())
                .w_full()
                .text_center()
                .font_family(self.typeface.font.family.clone())
                .text_size(size)
                .line_height(px(height))
                .text_color(color)
                .child(text)
        };
        root.gap(px(metrics.gap))
            .pt(px(metrics.top))
            .child(
                div()
                    .debug_selector(|| "fb1-state-mark".into())
                    .mb(px(metrics.under_mark))
                    .child(StateMark {
                        side: (f32::from(self.typeface.body_small) * 3.1).round(),
                        glyph: facts.glyph(),
                        ink: facts.mark_ink(ink),
                    }),
            )
            .child(line(
                "fb1-state-sentence",
                facts.sentence.clone(),
                self.typeface.body,
                metrics.sentence_line,
                ink.foreground,
            ))
            .when(!facts.detail.is_empty(), |root| {
                root.child(
                    line(
                        "fb1-state-detail",
                        facts.detail.clone(),
                        self.typeface.caption,
                        metrics.detail_line,
                        ink.muted,
                    )
                    .mt(px(metrics.detail_drop)),
                )
            })
    }
}
