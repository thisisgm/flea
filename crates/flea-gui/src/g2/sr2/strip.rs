//! FilterStrip as the filter boards draw it: 27px on the pane's ground, the accent funnel, the query and its caret, the summary, "esc clears".
use super::present::StripText;
#[path = "../../strips/filter_query.rs"]
mod query_paint;
use crate::list::{Palette, Typeface};
use gpui::{
    fill, point, px, relative, size, App, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, Pixels, ShapedLine, SharedString, Style, TextAlign,
    TextRun, TransformationMatrix, Window,
};
use query_paint::QueryLine;
use std::{cell::RefCell, rc::Rc};

// Board boards.py filter_strip: 27 tall, 14 padding, 9 gap, a 16 funnel, a 2 by 13 caret 1 after the query, a 12% rule.
pub const HEIGHT: f32 = 27.;
const PAD_X: f32 = 14.;
const GAP: f32 = 9.;
const MARK: f32 = 16.;
const CARET_W: f32 = 2.;
const CARET_H: f32 = 13.;
const CARET_GAP: f32 = 1.;
const RULE: f32 = 1.;
const RULE_ALPHA: f32 = 0.12;
const ELLIPSIS: &str = "\u{2026}";
const FUNNEL: (&str, &[u8]) = (
    "flea-mark:filter",
    br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="square" stroke-linejoin="miter"><path d="M3 4h18l-7 8v7l-4 2v-9z"/></svg>"#,
);

/// Where the last frame put each part, read by tests and the visual probe instead of pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Laid {
    pub mark: Bounds<Pixels>,
    pub query: Bounds<Pixels>,
    pub caret: Option<Bounds<Pixels>>,
    pub summary: Option<Bounds<Pixels>>,
    pub way_out: Bounds<Pixels>,
    pub rule: Bounds<Pixels>,
}

#[derive(Default)]
struct Shaped {
    key: Option<(SharedString, Hsla, Pixels, gpui::Font)>,
    line: Option<ShapedLine>,
}

impl Shaped {
    // Reshapes only when the words, ink, size or face change, so a frame at rest shapes nothing.
    fn get(
        &mut self,
        text: &SharedString,
        color: Hsla,
        size: Pixels,
        font: &gpui::Font,
        window: &Window,
    ) -> &ShapedLine {
        let key = (text.clone(), color, size, font.clone());
        if self.key.as_ref() != Some(&key) || self.line.is_none() {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            self.line = Some(
                window
                    .text_system()
                    .shape_line(text.clone(), size, &[run], None),
            );
            self.key = Some(key);
        }
        self.line.as_ref().expect("shaped above")
    }
}

/// The strip's state across frames: its words, the face, the shaped lines and where they went.
pub struct StripState {
    pub text: StripText,
    pub typeface: Typeface,
    pub(crate) query: QueryLine,
    pub(crate) input: Option<gpui::Entity<super::input::FilterInput>>,
    summary: Shaped,
    way_out: Shaped,
    // The summary cut to its room: the full words, room and face it was cut for, the cut words, and the ellipsis it measures.
    cut_for: Option<(SharedString, Pixels, gpui::Font)>,
    cut: Option<SharedString>,
    elided: Shaped,
    ellipsis: Shaped,
    pub laid: Option<Laid>,
    pub shaped: usize,
}

impl StripState {
    /// The summary words the last frame painted, cut and closed by an ellipsis when they overran their room.
    pub fn summary_drawn(&self) -> Option<&SharedString> {
        self.laid.and_then(|laid| laid.summary)?;
        let line = if self.cut.is_some() {
            &self.elided
        } else {
            &self.summary
        };
        line.key.as_ref().map(|key| &key.0)
    }

    pub fn new(typeface: Typeface) -> Self {
        Self {
            text: StripText::default(),
            typeface,
            query: QueryLine::default(),
            input: None,
            summary: Shaped::default(),
            way_out: Shaped::default(),
            cut_for: None,
            cut: None,
            elided: Shaped::default(),
            ellipsis: Shaped::default(),
            laid: None,
            shaped: 0,
        }
    }
}

pub struct StripElement {
    state: Rc<RefCell<StripState>>,
}

impl StripElement {
    pub fn new(state: Rc<RefCell<StripState>>) -> Self {
        Self { state }
    }
}

impl IntoElement for StripElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for StripElement {
    type RequestLayoutState = ();
    type PrepaintState = Palette;

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            size: size(relative(1.).into(), px(HEIGHT).into()),
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Palette {
        let palette = Palette::inherited(
            &window.text_style(),
            cx.try_global::<crate::palette::Theme>()
                .copied()
                .unwrap_or_default(),
        );
        let mut state = self.state.borrow_mut();
        let StripState {
            text,
            typeface,
            query,
            summary,
            way_out,
            cut_for,
            cut,
            elided,
            ellipsis,
            laid,
            shaped,
            input,
        } = &mut *state;
        let before = [&summary.key, &way_out.key, &elided.key, &ellipsis.key].map(Clone::clone);
        let query_before = query.shapings;
        let font = typeface.font.clone();
        let (left, top) = (bounds.origin.x, bounds.origin.y);
        let middle = |height: f32| top + px((HEIGHT - RULE - height) / 2.);
        let mark = Bounds::new(
            point(left + px(PAD_X), middle(MARK)),
            size(px(MARK), px(MARK)),
        );
        let query_x = mark.origin.x + px(MARK + GAP);
        let query_w = query.get(
            &text.query,
            palette.foreground,
            typeface.body,
            &font,
            window,
        );
        let query_box = Bounds::new(point(query_x, top), size(query_w, px(HEIGHT - RULE)));
        let mut end = query_x + query_w;
        let mut caret = text.caret.then(|| {
            let caret = Bounds::new(
                point(end + px(CARET_GAP), middle(CARET_H)),
                size(px(CARET_W), px(CARET_H)),
            );
            end = caret.origin.x + px(CARET_W);
            caret
        });
        let way_w = way_out
            .get(
                &text.way_out,
                palette.metadata,
                typeface.caption,
                &font,
                window,
            )
            .width();
        let right = left + bounds.size.width - px(PAD_X);
        let way_box = Bounds::new(point(right - way_w, top), size(way_w, px(HEIGHT - RULE)));
        let summary_box = (!text.summary.is_empty()).then(|| {
            let full = summary.get(
                &text.summary,
                palette.metadata,
                typeface.caption,
                &font,
                window,
            );
            let x = end + px(GAP);
            let room = (way_box.origin.x - px(GAP) - x).max(px(0.));
            let mut width = full.width();
            if width <= room {
                (*cut, *cut_for) = (None, None);
            } else {
                // FilterStrip.qml elides right: the words that fit before "esc clears", then an ellipsis, recut only when either changes.
                if cut_for.as_ref() != Some(&(text.summary.clone(), room, font.clone())) {
                    let dots = ellipsis
                        .get(
                            &SharedString::new_static(ELLIPSIS),
                            palette.metadata,
                            typeface.caption,
                            &font,
                            window,
                        )
                        .width();
                    let keep = full.index_for_x((room - dots).max(px(0.))).unwrap_or(0);
                    *cut = Some(format!("{}{ELLIPSIS}", text.summary[..keep].trim_end()).into());
                    *cut_for = Some((text.summary.clone(), room, font.clone()));
                }
                let words = cut.clone().unwrap_or_default();
                width = elided
                    .get(&words, palette.metadata, typeface.caption, &font, window)
                    .width()
                    .min(room);
            }
            Bounds::new(point(x, top), size(width, px(HEIGHT - RULE)))
        });
        let rule = Bounds::new(
            point(left, top + px(HEIGHT - RULE)),
            size(bounds.size.width, px(RULE)),
        );
        *shaped += [&summary.key, &way_out.key, &elided.key, &ellipsis.key]
            .iter()
            .zip(&before)
            .filter(|(now, was)| ***now != **was)
            .count()
            + query.shapings
            - query_before;
        if let Some(input) = input {
            let adapter = input.read(cx);
            let room = (way_box.left() - px(GAP) - query_box.left()).max(px(0.));
            caret = query.place(query_box, room, &adapter.entry.borrow());
        }
        *laid = Some(Laid {
            mark,
            query: query_box,
            caret,
            summary: summary_box,
            way_out: way_box,
            rule,
        });
        palette
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        palette: &mut Palette,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = self.state.borrow();
        let Some(laid) = state.laid else { return };
        window.paint_quad(fill(bounds, palette.background));
        window.paint_quad(fill(laid.rule, palette.foreground.opacity(RULE_ALPHA)));
        let _ = window.paint_svg(
            laid.mark,
            SharedString::new_static(FUNNEL.0),
            Some(FUNNEL.1),
            TransformationMatrix::unit(),
            palette.accent,
            cx,
        );
        let line_height = px(HEIGHT - RULE);
        // The query and its caret stay together in a mirrored window; an edited query moves as the line it draws.
        let query = state.query.unit(laid, state.input.is_some());
        window.with_unmirrored(query, |window| {
            state
                .query
                .paint(laid, palette, state.input.as_ref(), window, cx);
        });
        let summary_line = if state.cut.is_some() {
            &state.elided.line
        } else {
            &state.summary.line
        };
        if let (Some(line), Some(summary)) = (summary_line, laid.summary) {
            let _ = line.paint(
                summary.origin,
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
        if let Some(line) = &state.way_out.line {
            let _ = line.paint(
                laid.way_out.origin,
                line_height,
                TextAlign::Left,
                None,
                window,
                cx,
            );
        }
    }
}
