//! The Quick Look card: the ground, then each kind's surface as its board draws it.
use super::{
    bar::{self, mark, Name},
    glyph::Glyph,
    look::{self, Look},
    shield::{OnPointer, Shield},
    viewport::pane,
    Body, QuickLook, RasterRefusal,
};
use crate::row::marks::Mark;
use flea::ui_logic_pv1::scroll;
use gpui::{
    div, prelude::*, px, AnyElement, Bounds, Context, Hsla, Pixels, Render, SharedString, Window,
};
use std::{borrow::Cow, rc::Rc};

#[path = "card_diff.rs"]
mod diff;
use diff::diff_row;

#[cfg(test)]
#[path = "s5_motion_tests.rs"]
mod s5_tests;

impl Render for QuickLook {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window = Some(window.window_handle());
        self.forget_painted_bounds();
        self.drop_retired(window);
        if !self.visible() {
            return div().into_any_element();
        }
        let active = self.subject.is_some();
        let picture = self.subject.as_ref().is_some_and(|subject| {
            matches!(
                subject.body,
                Body::Image { .. } | Body::Link { .. } | Body::Raw { .. }
            )
        });
        // Decoding begins before the reveal clock, so the first raster gets the full fade and rise.
        if picture {
            let card = self.card(window.viewport_size());
            self.picture_raster(
                f64::from(f32::from(card.size.width)),
                f64::from(f32::from(card.size.height)),
                window,
                cx,
            );
        }
        let motion = self.motion.sample(
            active && (self.kept_shown || self.revealed && (!picture || self.content_ready(cx))),
            cx.background_executor().now(),
            cx.reduce_motion(),
        );
        if motion.animating {
            window.request_animation_frame();
        }
        if !active && !motion.visible {
            self.finish_close(cx);
            cx.notify();
            return div().into_any_element();
        }
        if !active {
            self.subject = self.closing.take();
        }
        self.observe_frames(window, cx);
        let theme = cx
            .try_global::<crate::palette::Theme>()
            .copied()
            .unwrap_or_default();
        let mut look = self.look(&window.text_style(), theme);
        if self.look.is_none() {
            let palette = crate::list::Palette::inherited(&window.text_style(), theme);
            look = look.with_frames(crate::presentation::frames::Frames::from_palette(
                &palette,
                crate::compositor::corner(cx),
            ));
        }
        let mut card = self.card(window.viewport_size());
        card.origin.y += px(motion.rise);
        match self.viewport(card, window) {
            Some(view) => self.scroll.fit(view),
            None => self.scroll.view = None,
        }
        let this = cx.entity().downgrade();
        let on_pointer: OnPointer = Rc::new(move |hand, _, cx| {
            let _ = this.update(cx, |ql, cx| ql.pointer(hand, cx));
        });
        // Unrevealed, the ground still takes every event but draws nothing, so the held listing shows through.
        let ground = look.scrim().opacity(motion.opacity);
        let mut overlay = div().absolute().size_full().when(active, |overlay| {
            overlay.child(
                Shield::new(card, ground, on_pointer)
                    .dragging(self.scroll.drag.is_some() || self.scroll.pan.is_some()),
            )
        });
        // A mounted view lays out unseen while held back, so it can ask for its first screen before the reveal.
        let laying = self.has_doc()
            || self.subject.as_ref().is_some_and(|subject| {
                matches!(
                    subject.body,
                    Body::Image { .. } | Body::Link { .. } | Body::Raw { .. }
                )
            });
        // A move draws the held card over the new subject until it reveals, then swaps in one frame.
        let holding = self.held.is_some() && !self.revealed;
        if self.revealed || laying || !active || (self.kept_shown && !holding) {
            let shown = if holding { 0. } else { motion.opacity };
            let surface = self.surface(card, None, &look, window, cx);
            overlay = overlay.child(div().absolute().size_full().opacity(shown).child(surface));
        }
        if holding {
            overlay = overlay.children(self.held_layer(motion, &look, window, cx));
        }
        if !active {
            self.closing = self.subject.take();
        }
        let ready = self.content_ready(cx);
        let landed = self.observations.borrow().landed;
        if self.revealed && self.content_landed(cx) && !landed {
            self.observe_reveal(false, cx);
        }
        let trace = self.trace.clone();
        if let Some(trace) = &trace {
            trace.observe(window, cx);
        }
        let full = trace.is_some() && self.full_quality(cx);
        let frames = self.observation_frames.clone();
        let stamp = {
            let observed = self.observations.borrow();
            super::observed_frames::Stamp {
                visible: self.revealed,
                ready,
                holding: observed.swap.holding,
                loading: self.revealed && !ready && observed.revealed_early,
            }
        };
        overlay = overlay.child(
            gpui::canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    if let Some(trace) = &trace {
                        trace.stage(window, stamp.visible && stamp.ready && !stamp.holding, full);
                    }
                    if let Some(id) = window.current_draw_id() {
                        frames.borrow_mut().stage(id, stamp);
                    }
                },
            )
            .absolute()
            .size_full(),
        );
        if self.custom.is_some() {
            overlay = overlay.bg(look.background).children(self.custom_chrome(window, cx));
        }
        overlay.into_any_element()
    }
}

pub(super) fn text(content: &str, size: f32, ink: Hsla) -> gpui::Div {
    div()
        .text_size(px(size))
        .text_color(ink)
        .whitespace_nowrap()
        .child(SharedString::from(content.to_owned()))
}

impl QuickLook {
    /// A held card passes its last raster, so drawing it asks for no new decode.
    pub(super) fn surface(
        &mut self,
        card: Bounds<Pixels>,
        raster: Option<super::RasterState>,
        look: &Look,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (w, h) = (
            f64::from(f32::from(card.size.width)),
            f64::from(f32::from(card.size.height)),
        );
        let raster = raster.unwrap_or_else(|| self.picture_raster(w, h, window, cx));
        let subject = self.subject.as_ref().expect("an open card has a subject");
        // A refused or unreadable host-decoded picture shows the kind's can't-show card, never img(path).
        let shown: Cow<'_, Body> = match (&subject.body, raster.refusal) {
            (Body::Image { .. } | Body::Link { .. } | Body::Raw { .. }, Some(refusal)) => {
                let too_large = refusal == RasterRefusal::TooLarge;
                match Body::cannot_show(
                    &self.i18n,
                    subject.state,
                    too_large,
                    !too_large,
                    Vec::new(),
                ) {
                    Some(body) => Cow::Owned(body),
                    None => Cow::Borrowed(&subject.body),
                }
            }
            // A refused code read draws the calm card in the view's place, as a refused picture does.
            _ => match self.calm_body(cx) {
                Some(body) => Cow::Owned(body),
                None => Cow::Borrowed(&subject.body),
            },
        };
        let mut surface = div()
            .id("pv1-card")
            .debug_selector(|| "pv1-card".into())
            .absolute()
            .left(card.origin.x)
            .top(card.origin.y)
            .w(card.size.width)
            .h(card.size.height)
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(super::observations::bounds(
                &self.observations,
                |observed, bounds| {
                    observed.surface = Some(bounds);
                },
            ));
        surface = if shown.framed(cx) {
            surface
                .bg(look.surface)
                .border_1()
                .border_color(look.muted_frame)
                .rounded(look.corner)
        } else {
            surface.bg(crate::window::glass::ground(look.background, window, cx))
        };
        surface = surface.children(crate::g2::pv2::html::capture::frame(
            subject.body.doc().and_then(|doc| match &doc.view {
                super::doc::View::Html(view) => Some(view.clone()),
                _ => None,
            }),
            1.,
            look.muted_frame,
            look.surface,
        ));
        let mark_of = subject.mark_with_languages(self.languages);
        let children: Vec<AnyElement> = match shown.as_ref() {
            Body::Text { lines, count } => {
                let name = Name {
                    text: &subject.name,
                    tail: Some((count, look.muted)),
                };
                let mut parts = vec![bar::bar(mark_of, name, None, false, look, self, cx)];
                if let Some(view) = self.scroll.view {
                    let y = self.scroll.y;
                    let (first, last) = scroll::rows_in_view(
                        y,
                        0.,
                        f64::from(look::TEXT_LINE),
                        f64::from(f32::from(view.bounds.size.height)),
                        lines.len(),
                    );
                    let rows = (first..last).map(|i| {
                        div()
                            .debug_selector(move || format!("pv1-line-{i}"))
                            .absolute()
                            .left(px(look::TEXT_LEFT - look::PAD))
                            .right(px(look::TEXT_RIGHT - look::PAD))
                            .top(px((i as f64 * f64::from(look::TEXT_LINE) - y) as f32))
                            .h(px(look::TEXT_LINE))
                            .line_height(px(look::TEXT_LINE))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(text(&lines[i], look::BODY, look.foreground))
                            .into_any_element()
                    });
                    let pane = pane(look::TEXT_TOP, look::PAD, look::TEXT_VIEW_BOTTOM)
                        .child(super::observations::bounds(
                            &self.observations,
                            |observed, bounds| observed.surface = Some(bounds),
                        ))
                        .children(rows)
                        .children(self.scroll.bars(look));
                    // File content keeps its own layout in a mirrored window.
                    parts.push(crate::locale_geometry::unmirrored(pane).into_any_element());
                }
                parts
            }
            Body::Image { .. } | Body::Link { .. } | Body::Raw { .. } => {
                self.picture_parts(subject, w, h, &raster, look, cx)
            }
            Body::CannotShow {
                sentence, error, ..
            } => {
                let mut parts = Vec::new();
                let (glyph, mark_ink, ink, top) = if *error {
                    let name = Name {
                        text: &subject.name,
                        tail: None,
                    };
                    parts.push(bar::bar(mark_of, name, None, true, look, self, cx));
                    (Glyph::Alert, look.error, look.foreground, look::BAR)
                } else {
                    (Glyph::Row(Mark::File), look.meta, look.meta, 0.)
                };
                let button = super::button::open_with(self, look, window, cx);
                parts.push(recipe(glyph, mark_ink, sentence, ink, Some(button), top));
                parts
            }
            Body::Diff { title, copy, rows } => {
                let name = Name {
                    text: title,
                    tail: None,
                };
                let head = bar::bar(mark_of, name, Some(copy), false, look, self, cx);
                let mut parts = vec![div()
                    .absolute()
                    .left(px(-1.))
                    .top(px(-1.))
                    .right(px(-1.))
                    .child(head)
                    .into_any_element()];
                if let Some(view) = self.scroll.view {
                    let (y, x) = (self.scroll.y, self.scroll.x);
                    let (first, last) = scroll::rows_in_view(
                        y,
                        f64::from(look::DIFF_MARGIN),
                        f64::from(look::DIFF_LINE),
                        f64::from(f32::from(view.bounds.size.height)),
                        rows.len(),
                    );
                    let width = view
                        .content_w
                        .max(f64::from(f32::from(view.bounds.size.width)));
                    let lines = (first..last).map(|i| diff_row(i, &rows[i], look, (x, y), width));
                    let pane = pane(look::DIFF_VIEW_TOP, 0., 0.)
                        .children(lines)
                        .children(self.scroll.bars(look));
                    // File content keeps its own layout in a mirrored window.
                    parts.push(crate::locale_geometry::unmirrored(pane).into_any_element());
                }
                parts
            }
            Body::Doc(doc) => self.doc_parts(subject, doc, look, window, cx),
            // A Compare whose diff is still out draws its bar alone, and the waiting mark after the hold.
            Body::Refused { title, sentence } if sentence.is_empty() => {
                vec![bar::refusal_bar(mark_of, title, look, self, cx)]
            }
            Body::Refused { title, sentence } => vec![
                bar::refusal_bar(mark_of, title, look, self, cx),
                recipe(
                    Glyph::Row(Mark::File),
                    look.meta,
                    sentence,
                    look.meta,
                    None,
                    look::REFUSAL_BAR + 1.,
                ),
            ],
        };
        surface
            .children(children)
            .children(self.waiting_mark(look, cx))
            .into_any_element()
    }
}

/// The can't-show recipe: the 40 px mark, the 14 px sentence 9 px under it, and the button 9 px more down.
pub(super) fn recipe(
    glyph: Glyph,
    mark_ink: Hsla,
    sentence: &str,
    ink: Hsla,
    button: Option<AnyElement>,
    top: f32,
) -> AnyElement {
    div()
        .id("pv1-recipe")
        .debug_selector(|| "pv1-recipe".into())
        .absolute()
        .left_0()
        .right_0()
        .top(px(top))
        .bottom_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .line_height(gpui::relative(1.2))
        .gap(px(look::GAP))
        .child(mark(glyph, look::MARK, mark_ink))
        .child(text(sentence, look::BODY, ink))
        .children(button)
        .into_any_element()
}
