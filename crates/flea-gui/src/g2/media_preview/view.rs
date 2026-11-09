//! The picture area above the strip: the fitted frame on a letterbox, the poster mark, the audio pane or the failure.
use super::glyphs::Glyph;
use super::ink::{size, MediaInk};
use crate::list::Typeface;
use flea::ui_logic_media_preview::picture::fit;
use gpui::{
    fill, point, px, relative, App, Bounds, Corners, Element, ElementId, Font, GlobalElementId,
    GlyphRun, Hsla, ImageId, InspectorElementId, IntoElement, LayoutId, Pixels, RenderImage, Style,
    TextRun, TransformationMatrix, Window,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// What fills the area.
#[derive(Clone, Debug, PartialEq)]
pub enum Picture {
    /// A video: its newest frame once one decoded, and whether the stopped play mark shows over it.
    Video {
        frame: Option<Arc<RenderImage>>,
        poster: bool,
    },
    /// Audio: the kind's mark beside the name and its facts line.
    Audio { name: String, facts: String },
    /// The failure sentence under the alert mark.
    Failed { sentence: String },
}

/// Lines shaped once per text, so a playing frame shapes nothing.
#[derive(Default)]
pub struct ViewCache {
    lines: Vec<(String, f32, Rc<GlyphRun>)>,
}

impl ViewCache {
    fn line(&mut self, text: &str, font_size: f32, font: &Font, window: &Window) -> Rc<GlyphRun> {
        if let Some((_, _, run)) = self
            .lines
            .iter()
            .find(|(shown, at, _)| shown == text && *at == font_size)
        {
            return run.clone();
        }
        let run = TextRun {
            len: text.len(),
            font: font.clone(),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = Rc::new(GlyphRun::new(window.text_system().layout_line(
            text,
            px(font_size),
            &[run],
            None,
        )));
        // One subject draws at most three lines; a new subject replaces them.
        if self.lines.len() >= 3 {
            self.lines.clear();
        }
        self.lines
            .push((text.to_string(), font_size, shaped.clone()));
        shaped
    }
}

/// Cuts a line to the cells a column holds, ending it in an ellipsis, as Text.ElideRight does.
pub fn elide_right(text: &str, cells: usize) -> String {
    if text.chars().count() <= cells {
        return text.to_string();
    }
    let mut kept: String = text.chars().take(cells.saturating_sub(1)).collect();
    kept.push('\u{2026}');
    kept
}

/// What a player's pictures hold in the atlas, shared by the player and the area that paints them.
#[derive(Default)]
pub struct Painted {
    /// The picture last uploaded by a paint, until the player lets it go.
    pub uploaded: Option<ImageId>,
    /// The uploaded picture a newer one replaced; the next paint frees it, after the frame that showed it.
    pub retired: Option<Arc<RenderImage>>,
}

pub type Shown = Rc<RefCell<Painted>>;

pub struct MediaElement {
    pub picture: Picture,
    /// The strip height a Quick Look card keeps under its video area; 0 inside the column's frame.
    pub reserve: f32,
    pub ink: MediaInk,
    pub typeface: Typeface,
    pub cache: Rc<RefCell<ViewCache>>,
    pub painted: Shown,
}

/// Lines the paint draws, shaped in prepaint, each with the line height it sits in.
pub struct Shaped {
    lines: Vec<(Rc<GlyphRun>, f32)>,
}

impl IntoElement for MediaElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn mark(bounds: Bounds<Pixels>, glyph: Glyph, ink: Hsla, window: &mut Window, cx: &mut App) {
    let (key, data) = glyph.svg();
    let _ = window.paint_svg(
        bounds,
        key,
        Some(data),
        TransformationMatrix::unit(),
        ink,
        cx,
    );
}

fn square(x: Pixels, y: Pixels, side: f32) -> Bounds<Pixels> {
    Bounds::new(point(x, y), gpui::size(px(side), px(side)))
}

impl Element for MediaElement {
    type RequestLayoutState = ();
    type PrepaintState = Shaped;

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
            size: gpui::size(relative(1.).into(), relative(1.).into()),
            flex_grow: 1.,
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
        _: &mut App,
    ) -> Shaped {
        let mut cache = self.cache.borrow_mut();
        let lines = match &self.picture {
            Picture::Video { .. } => Vec::new(),
            Picture::Audio { name, facts } => {
                // PreviewMedia's text column: the area less its row padding on both sides, the mark and the gap.
                let column = audio_column(f32::from(bounds.size.width));
                let mut shaped = Vec::new();
                let (body, caption) = (
                    f32::from(self.typeface.body),
                    f32::from(self.typeface.caption),
                );
                for (text, at, height) in [
                    (name, body, size::BODY_LINE),
                    (facts, caption, size::CAPTION_LINE),
                ] {
                    if text.is_empty() {
                        continue;
                    }
                    let whole = cache.line(text, at, &self.typeface.font, window);
                    let advance =
                        f32::from(whole.line().width) / text.chars().count().max(1) as f32;
                    let cells = (column.max(0.) / advance.max(1.)).floor() as usize;
                    let run = if text.chars().count() > cells {
                        cache.line(&elide_right(text, cells), at, &self.typeface.font, window)
                    } else {
                        whole
                    };
                    shaped.push((run, height));
                }
                shaped
            }
            Picture::Failed { sentence } => vec![(
                cache.line(
                    sentence,
                    f32::from(self.typeface.caption),
                    &self.typeface.font,
                    window,
                ),
                size::CAPTION_LINE,
            )],
        };
        Shaped { lines }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        shaped: &mut Shaped,
        window: &mut Window,
        cx: &mut App,
    ) {
        let ink = self.ink;
        let centre = bounds.center();
        if let Some(retired) = self.painted.borrow_mut().retired.take() {
            let _ = window.drop_image(retired);
        }
        match &self.picture {
            Picture::Video { frame, poster } => {
                // ql-video.html: the picture area stops at the strip's top edge, so its marks centre above it.
                let bounds = Bounds::new(
                    bounds.origin,
                    gpui::size(
                        bounds.size.width,
                        (bounds.size.height - px(self.reserve)).max(px(0.)),
                    ),
                );
                let centre = bounds.center();
                // The letterbox is the canvas's own darker ground, not the surface the overlay sits on.
                window.paint_quad(fill(bounds, ink.background));
                if let Some(frame) = frame {
                    let picture = frame.size(0);
                    let [x, y, w, h] = fit(
                        picture.width.0 as f32,
                        picture.height.0 as f32,
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                    );
                    let placed = Bounds::new(
                        point(bounds.origin.x + px(x), bounds.origin.y + px(y)),
                        gpui::size(px(w), px(h)),
                    );
                    let _ = window.paint_image(
                        placed,
                        placed,
                        Corners::default(),
                        frame.clone(),
                        0,
                        false,
                    );
                    self.painted.borrow_mut().uploaded = Some(frame.id);
                }
                if *poster {
                    let half = px(size::STATE_MARK / 2.);
                    let at = square(centre.x - half, centre.y - half, size::STATE_MARK);
                    mark(
                        at,
                        Glyph::Play,
                        ink.muted_frame.opacity(size::STATE_MARK_OPACITY),
                        window,
                        cx,
                    );
                }
            }
            Picture::Audio { .. } => {
                // ql-audio.html: the mark and a fixed text column centre on the whole card, under the strip too.
                let column = audio_column(f32::from(bounds.size.width));
                let left = centre.x - px(size::STATE_MARK + size::AUDIO_GAP + column) / 2.;
                let half = px(size::STATE_MARK / 2.);
                let at = square(left, centre.y - half, size::STATE_MARK);
                mark(
                    at,
                    Glyph::Music,
                    ink.muted_frame.opacity(size::STATE_MARK_OPACITY),
                    window,
                    cx,
                );
                let heights: f32 = shaped.lines.iter().map(|(_, height)| *height).sum();
                let mut top = centre.y - px(heights / 2.);
                for (index, (run, height)) in shaped.lines.iter().enumerate() {
                    let colour = if index == 0 {
                        ink.foreground
                    } else {
                        ink.muted
                    };
                    paint_centred_line(
                        run,
                        left + px(size::STATE_MARK + size::AUDIO_GAP),
                        top,
                        *height,
                        colour,
                        window,
                    );
                    top += px(*height);
                }
            }
            Picture::Failed { .. } => {
                let Some((run, _)) = shaped.lines.first() else {
                    return;
                };
                let total = size::STATE_MARK + size::GAP + size::CAPTION_LINE;
                let top = centre.y - px(total / 2.);
                let half = px(size::STATE_MARK / 2.);
                mark(
                    square(centre.x - half, top, size::STATE_MARK),
                    Glyph::Alert,
                    ink.error,
                    window,
                    cx,
                );
                let x = centre.x - run.line().width / 2.;
                paint_centred_line(
                    run,
                    x,
                    top + px(size::STATE_MARK + size::GAP),
                    size::CAPTION_LINE,
                    ink.foreground,
                    window,
                );
            }
        }
    }
}

/// PreviewMedia's text column: the width less row padding on both sides, the mark and the gap; 820 on the 902 card.
pub fn audio_column(width: f32) -> f32 {
    (width - 2. * size::ROW_PAD - size::STATE_MARK - size::AUDIO_GAP).max(0.)
}

fn paint_centred_line(
    run: &GlyphRun,
    x: Pixels,
    top: Pixels,
    height: f32,
    colour: Hsla,
    window: &mut Window,
) {
    let line = run.line();
    let baseline = top + (px(height) - line.ascent - line.descent) / 2. + line.ascent;
    let _ = window.paint_glyph_run(
        point(window.pixel_snap(x), window.pixel_snap(baseline)),
        run,
        colour,
    );
}
