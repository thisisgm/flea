//! The Quick Look card: the ground, then each kind's surface as its board draws it.
use super::{
    bar::{self, mark, Name},
    glyph::Glyph,
    look::{self, Look},
    viewport::pane,
    shield::{OnPointer, Shield},
    Body, QuickLook, RasterRefusal, RasterState,
};
use crate::{list::Palette, row::marks::Mark};
use flea::ui_logic_pv1::{
    compare::{DiffRow, Kind},
    pixels::is_vector_name,
    scroll,
};
use gpui::{
    div, img, prelude::*, px, AnyElement, Bounds, Context, Hsla, ImageSource, ObjectFit, Pixels,
    Render, RenderImage, SharedString, Window,
};
use std::{borrow::Cow, path::Path, rc::Rc, sync::Arc};

impl Render for QuickLook {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.window = Some(window.window_handle());
        for image in std::mem::take(&mut self.retired) {
            let _ = window.drop_image(image);
        }
        if self.subject.is_none() {
            return div().into_any_element();
        }
        let look = self
            .look
            .unwrap_or_else(|| Look::inherited(&Palette::inherited(&window.text_style())));
        let card = self.card(window.viewport_size());
        match self.viewport(card, window) {
            Some(view) => self.scroll.fit(view),
            None => self.scroll.view = None,
        }
        let this = cx.entity().downgrade();
        let on_pointer: OnPointer = Rc::new(move |hand, _, cx| {
            let _ = this.update(cx, |ql, cx| ql.pointer(hand, cx));
        });
        // Unrevealed, the ground still takes every event but draws nothing, so the held listing shows through.
        let ground = if self.revealed {
            look.scrim()
        } else {
            look.scrim().opacity(0.)
        };
        let mut overlay = div()
            .absolute()
            .size_full()
            .child(Shield::new(card, ground, on_pointer).dragging(self.scroll.drag.is_some()));
        if self.revealed {
            overlay = overlay.child(self.surface(card, &look, window, cx));
        }
        overlay.into_any_element()
    }
}

fn picture(path: &Path, at: (f64, f64), w: f64, h: f64, raster: Option<Arc<RenderImage>>) -> AnyElement {
    // A vector draws only its own raster, or nothing while pending; a raster file decodes whole from its file.
    let vector = path.file_name().and_then(|n| n.to_str()).is_some_and(is_vector_name);
    let source = raster
        .map(ImageSource::from)
        .or_else(|| (!vector).then(|| ImageSource::from(path.to_path_buf())));
    let mut picture = div()
        .absolute()
        .left(px(at.0 as f32))
        .top(px(at.1 as f32))
        .w(px(w as f32))
        .h(px(h as f32))
        .debug_selector(|| "pv1-picture".into());
    if let Some(source) = source {
        picture = picture.child(img(source).size_full().object_fit(ObjectFit::Fill));
    }
    picture.into_any_element()
}

pub(super) fn text(content: &str, size: f32, ink: Hsla) -> gpui::Div {
    div()
        .text_size(px(size))
        .text_color(ink)
        .whitespace_nowrap()
        .child(SharedString::from(content.to_owned()))
}

impl QuickLook {
    /// The open picture's vector raster, asked before the subject borrow below takes `&self` (#336).
    fn vector_raster(
        &mut self,
        w: f64,
        h: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> RasterState {
        let next = self.subject.as_ref().and_then(|s| match &s.body {
            Body::Image { path, w: pw, h: ph } => {
                super::body::fit_rounded(w, h, *pw, *ph).map(|f| (path.clone(), f.w, f.h))
            }
            Body::Link { path, w: pw, h: ph, .. } => {
                let room = h - f64::from(look::BAR);
                super::body::fit_painted(w, room, *pw, *ph).map(|f| (path.clone(), f.w, f.h))
            }
            Body::Raw { path, w: pw, h: ph, .. } => {
                let room = h - f64::from(look::BAR);
                super::body::fit_rounded(w, room, *pw, *ph).map(|f| (path.clone(), f.w, f.h))
            }
            _ => None,
        });
        match next {
            Some((path, fw, fh)) => self.svg_image(&path, fw, fh, window, cx),
            None => RasterState::default(),
        }
    }

    fn surface(
        &mut self,
        card: Bounds<Pixels>,
        look: &Look,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (w, h) = (
            f64::from(f32::from(card.size.width)),
            f64::from(f32::from(card.size.height)),
        );
        let raster = self.vector_raster(w, h, window, cx);
        let subject = self.subject.as_ref().expect("an open card has a subject");
        // A refused or unreadable vector shows the kind's can't-show card, never a file GPUI would raster unbounded.
        let shown: Cow<'_, Body> = match (&subject.body, raster.refusal) {
            (Body::Image { path, .. } | Body::Link { path, .. } | Body::Raw { path, .. }, Some(refusal))
                if path.file_name().and_then(|n| n.to_str()).is_some_and(is_vector_name) =>
            {
                let too_large = refusal == RasterRefusal::TooLarge;
                match Body::cannot_show(&self.i18n, subject.state, too_large, !too_large, Vec::new()) {
                    Some(body) => Cow::Owned(body),
                    None => Cow::Borrowed(&subject.body),
                }
            }
            _ => Cow::Borrowed(&subject.body),
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
            .overflow_hidden();
        surface = if shown.framed() {
            surface
                .bg(look.surface)
                .border_1()
                .border_color(look.muted_frame)
                .rounded(look.corner)
        } else {
            surface.bg(look.background)
        };
        let mark_of = subject.mark();
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
                        .children(rows)
                        .children(self.scroll.bars(look));
                    parts.push(pane.into_any_element());
                }
                parts
            }
            Body::Image { path, w: pw, h: ph } => super::body::fit_rounded(w, h, *pw, *ph)
                .map(|f| vec![picture(path, (f.x, f.y), f.w, f.h, raster.image.clone())])
                .unwrap_or_default(),
            Body::Link {
                path,
                w: pw,
                h: ph,
                target,
            } => {
                let name = Name {
                    text: &subject.name,
                    tail: Some((target, look.meta)),
                };
                let mut parts = vec![bar::bar(mark_of, name, None, true, look, self, cx)];
                let room = h - f64::from(look::BAR);
                if let Some(f) = super::body::fit_painted(w, room, *pw, *ph) {
                    parts.push(picture(
                        path,
                        (f.x, f64::from(look::BAR) + f.y),
                        f.w,
                        f.h,
                        raster.image.clone(),
                    ));
                }
                parts
            }
            Body::Raw {
                path,
                w: pw,
                h: ph,
                caption,
            } => {
                let room = h - f64::from(look::BAR);
                let mut parts: Vec<AnyElement> = super::body::fit_rounded(w, room, *pw, *ph)
                    .map(|f| vec![picture(path, (f.x, f.y), f.w, f.h, raster.image.clone())])
                    .unwrap_or_default();
                parts.push(
                    div()
                        .debug_selector(|| "pv1-caption".into())
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px(look::BAR))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(text(caption, look::CAPTION, look.muted))
                        .into_any_element(),
                );
                parts
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
                    let width = view.content_w.max(f64::from(f32::from(view.bounds.size.width)));
                    let lines = (first..last).map(|i| diff_row(i, &rows[i], look, (x, y), width));
                    let pane = pane(look::DIFF_VIEW_TOP, 0., 0.)
                        .children(lines)
                        .children(self.scroll.bars(look));
                    parts.push(pane.into_any_element());
                }
                parts
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
        surface.children(children).into_any_element()
    }
}

/// The can't-show recipe: the 40 px mark, the 14 px sentence 9 px under it, and the button 9 px more down.
fn recipe(
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
        .gap(px(look::GAP))
        .child(mark(glyph, look::MARK, mark_ink))
        .child(text(sentence, look::BODY, ink))
        .children(button)
        .into_any_element()
}

/// One patch row at the view's position: the two muted numbers, the marker and the text in the row's ink; a hunk header sits at the text's column alone.
fn diff_row(index: usize, row: &DiffRow, look: &Look, (x, y): (f64, f64), width: f64) -> AnyElement {
    let at = f64::from(look::DIFF_MARGIN) + index as f64 * f64::from(look::DIFF_LINE) - y;
    let top = px(at as f32);
    let ink = match row.kind {
        Kind::Removed => look.error,
        Kind::Added => look.green,
        Kind::Hunk => look.cyan,
        Kind::Note => look.muted,
        Kind::Context => look.foreground,
    };
    let cell = |left: f32| div().absolute().left(px(left)).top_0();
    let mut line = div()
        .debug_selector(move || format!("pv1-diff-{index}"))
        .absolute()
        .left(px(-x as f32))
        .w(px(width as f32))
        .top(top)
        .h(px(look::DIFF_LINE))
        .line_height(px(look::DIFF_LINE))
        .text_size(px(look::BODY))
        .whitespace_nowrap()
        .text_color(ink);
    if matches!(row.kind, Kind::Context | Kind::Removed | Kind::Added) {
        let number = |n: u64, left: f32| {
            cell(left)
                .w(px(look::DIFF_NUMBER_W))
                .flex()
                .justify_end()
                .text_color(look.muted)
                .child(if n > 0 { n.to_string() } else { String::new() })
        };
        let marker = match row.kind {
            Kind::Removed => "-",
            Kind::Added => "+",
            _ => " ",
        };
        line = line
            .child(number(row.old_no, look::DIFF_OLD))
            .child(number(row.new_no, look::DIFF_NEW))
            .child(cell(look::DIFF_MARKER).child(marker));
    }
    line.child(cell(look::DIFF_TEXT).child(SharedString::from(row.text.clone())))
        .into_any_element()
}
