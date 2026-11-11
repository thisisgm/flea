//! Quick Look's font page (ui/FontPane.qml): the backend's three samples drawn at their logical size from their own
//! pixels, over the muted facts line, all appearing at once; a refused font draws only the unsupported sentence.
use super::code::shape::{self, Elide};
use super::code::{KindBar, Mark};
use super::html::decode;
use flea::i18n::I18n;
use flea::ui_logic_doc_preview::font::{self, Page, Reply};
use flea::ui_logic_doc_preview::refusal::Refusal;
use gpui::{
    canvas, point, prelude::*, px, size, Bounds, ContentMask, Context, Font, GlyphRun, Hsla,
    Pixels, Render, RenderImage, Task, Window,
};
use std::{cell::Cell, rc::Rc, sync::Arc};

/// One decoded sample and its pixel size.
pub type Decoded = (Arc<RenderImage>, (u32, u32));
/// A sample's logical rect: x, y, width, height from the frame's top left.
pub type Rect = (f32, f32, f32, f32);
/// The samples' rects and the facts line's top, for one pane and scale.
pub type Placed = (Vec<Rect>, f32);

/// The backend's `fontlook` route; its `fontlooked` answer comes back through `FontView::answered`.
pub trait FontHost {
    fn font_look(&self, ask: &font::Ask);
}

/// The theme the samples are drawn in: the foreground on the card's own colour, as hex, so the images carry no box.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub foreground: String,
    pub surface: String,
    pub muted: Hsla,
    /// Omarchy's text size over the stock size, which scales the board's sample sizes and spacing.
    pub size_ratio: f64,
}

/// The caption face of the facts line.
#[derive(Clone, Debug, PartialEq)]
pub struct Metrics {
    pub font: Font,
    pub caption: Pixels,
}

/// Three decoded samples of one path and their pixel sizes.
pub struct Samples {
    pub path: String,
    pub images: Vec<Decoded>,
}

pub struct FontView {
    pub page: Page,
    host: Option<Box<dyn FontHost>>,
    pub theme: Theme,
    pub metrics: Metrics,
    pub mark: Mark,
    pub shown: Option<Samples>,
    released: Vec<Arc<RenderImage>>,
    decode: Option<Task<()>>,
    catalog: Option<Arc<I18n>>,
    facts: Option<(String, Rc<GlyphRun>)>,
    /// The facts line's width and the samples' placement, computed at layout for the pane they were laid out in.
    facts_room: f32,
    placed: Option<((u32, u32, u32), Rc<Placed>)>,
    pub painted: Cell<usize>,
    pub decoded: usize,
}

impl FontView {
    pub fn new(theme: Theme, metrics: Metrics, mark: Mark) -> Self {
        FontView {
            page: Page::default(),
            host: None,
            theme,
            metrics,
            mark,
            shown: None,
            released: Vec::new(),
            decode: None,
            catalog: None,
            facts: None,
            facts_room: 0.,
            placed: None,
            painted: Cell::new(0),
            decoded: 0,
        }
    }

    pub fn set_host(&mut self, host: Option<Box<dyn FontHost>>) {
        self.host = host;
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.catalog = Some(catalog);
        self.facts = None;
        cx.notify();
    }

    pub fn set_subject(&mut self, path: &str, size: u64, cx: &mut Context<Self>) {
        self.page.path = path.to_string();
        self.page.size = size;
        self.facts = None;
        cx.notify();
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.page.active = active;
        cx.notify();
    }

    /// A theme load: the next frame asks once with both colours new, and the old samples stay until the new three land.
    pub fn set_theme(&mut self, theme: Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Sends the ask the current path, colours and sizes need, at most once per change of any of them.
    pub fn refresh(&mut self, scale: f32) {
        let sizes = font::physical(self.theme.size_ratio, f64::from(scale));
        if let (Some(ask), Some(host)) = (
            self.page
                .ask(&self.theme.foreground, &self.theme.surface, sizes),
            &self.host,
        ) {
            host.font_look(&ask);
        }
    }

    /// One answer: a refusal drops the page at once; a good one decodes its three files off the UI thread together.
    pub fn answered(&mut self, reply: Reply, cx: &mut Context<Self>) {
        if !self.page.answered(reply) {
            return;
        }
        self.facts = None;
        let Some(look) = self.page.look.clone() else {
            if let Some(old) = self.shown.take() {
                self.released
                    .extend(old.images.into_iter().map(|(image, _)| image));
            }
            self.decode = None;
            cx.notify();
            return;
        };
        let files = look.reply.files.clone();
        let job = cx.background_executor().spawn(async move {
            files
                .iter()
                .map(|file| decode::png(file))
                .collect::<Option<Vec<_>>>()
        });
        self.decode = Some(cx.spawn(async move |this, cx| {
            let images = job.await;
            let _ = this.update(cx, |view, cx| view.decoded(look.path, images, cx));
        }));
    }

    fn decoded(&mut self, path: String, images: Option<Vec<Decoded>>, cx: &mut Context<Self>) {
        self.decode = None;
        self.placed = None;
        match images {
            Some(images)
                if self
                    .page
                    .look
                    .as_ref()
                    .is_some_and(|look| look.path == path) =>
            {
                self.decoded += 1;
                if let Some(old) = self.shown.replace(Samples { path, images }) {
                    self.released
                        .extend(old.images.into_iter().map(|(image, _)| image));
                }
            }
            Some(_) => {}
            None => {
                self.page.samples_failed(&path);
                if let Some(old) = self.shown.take() {
                    self.released
                        .extend(old.images.into_iter().map(|(image, _)| image));
                }
            }
        }
        cx.notify();
    }

    /// The page shows only its own font, whole: the answer landed and its three samples decoded.
    pub fn drawn(&self) -> bool {
        self.page.drawn()
            && self
                .shown
                .as_ref()
                .is_some_and(|shown| shown.path == self.page.path)
    }

    pub fn refusal(&self) -> Option<Refusal> {
        self.page.refusal()
    }

    /// "loading" until the page draws, "font" once it does, or the unsupported sentence.
    pub fn status(&self, i18n: &I18n) -> String {
        match self.refusal().and_then(Refusal::status_key) {
            Some(key) => i18n.tr(key),
            None if self.drawn() => "font".into(),
            None => "loading".into(),
        }
    }

    pub fn fail_sentence(&self, i18n: &I18n) -> Option<String> {
        self.refusal()
            .and_then(Refusal::sentence_key)
            .map(|key| i18n.tr(key))
    }

    /// Quick Look's bar for a font: the type mark, the name and close; no count, no view words.
    pub fn bar(&self, i18n: &I18n) -> KindBar {
        KindBar::new(self.mark.clone(), &self.page.path, None, None, i18n)
    }

    /// The samples' logical rects and the facts line's top, from the frame's top left; None until the page draws.
    pub fn rects(&self, pane: (f32, f32), scale: f32, facts_height: f32) -> Option<Placed> {
        let shown = self.shown.as_ref().filter(|_| self.drawn())?;
        let sizes: Vec<(f32, f32)> = shown
            .images
            .iter()
            .map(|(_, (w, h))| (*w as f32 / scale, *h as f32 / scale))
            .collect();
        let ratio = self.theme.size_ratio;
        let gap = (font::SPACING * ratio).round() as f32;
        let block =
            sizes.iter().map(|(_, h)| h).sum::<f32>() + facts_height + gap * sizes.len() as f32;
        let (x, _, top, gap) = font::layout(
            (f64::from(pane.0), f64::from(pane.1)),
            f64::from(block),
            ratio,
        );
        let (x, gap) = (x as f32, gap as f32);
        let mut y = top as f32;
        let rects = sizes
            .into_iter()
            .map(|(w, h)| {
                let rect = (x, y, w, h);
                y += h + gap;
                rect
            })
            .collect();
        Some((rects, y))
    }

    fn layout(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &Window,
    ) -> (Vec<Arc<RenderImage>>, Option<Rc<GlyphRun>>) {
        let scale = window.scale_factor();
        self.refresh(scale);
        let pane = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        let room = pane.0 - 2. * (font::SIDE * self.theme.size_ratio).round() as f32;
        if room != self.facts_room {
            self.facts_room = room;
            self.facts = None;
        }
        if self.facts.is_none() && self.drawn() {
            if let Some(catalog) = &self.catalog {
                let text = self.page.facts_line(catalog);
                let (kept, run) = shape::elided(
                    window,
                    &text,
                    &self.metrics.font,
                    self.metrics.caption,
                    px(room),
                    Elide::End,
                );
                self.facts = Some((kept, Rc::new(run)));
                self.placed = None;
            }
        }
        let key = (pane.0.to_bits(), pane.1.to_bits(), scale.to_bits());
        if self.placed.as_ref().is_none_or(|(at, _)| *at != key) {
            let height = self.facts.as_ref().map_or(0., |(_, run)| {
                f32::from(run.line().ascent + run.line().descent)
            });
            self.placed = self
                .rects(pane, scale, height)
                .map(|placed| (key, Rc::new(placed)));
        }
        (
            std::mem::take(&mut self.released),
            self.facts.as_ref().map(|(_, run)| Rc::clone(run)),
        )
    }

    /// The facts line's drawn width, for the probes.
    pub fn facts_width(&self) -> Option<f32> {
        self.facts
            .as_ref()
            .map(|(_, run)| f32::from(run.line().width))
    }

    /// The facts line as drawn, for the probes.
    pub fn facts_text(&self) -> Option<&str> {
        self.facts.as_ref().map(|(text, _)| text.as_str())
    }

    fn paint(&self, bounds: Bounds<Pixels>, facts: Option<&GlyphRun>, window: &mut Window) {
        self.painted.set(self.painted.get() + 1);
        let (Some(shown), Some((_, placed))) =
            (self.shown.as_ref().filter(|_| self.drawn()), &self.placed)
        else {
            return;
        };
        let (rects, facts_top) = placed.as_ref();
        for ((x, y, w, h), (image, _)) in rects.iter().zip(&shown.images) {
            let rect = Bounds::new(bounds.origin + point(px(*x), px(*y)), size(px(*w), px(*h)));
            let _ = window.paint_image(rect, rect, Default::default(), Arc::clone(image), 0, false);
        }
        if let Some(run) = facts {
            let x = bounds.origin.x + px((font::SIDE * self.theme.size_ratio).round() as f32);
            let y = bounds.origin.y + px(*facts_top) + run.line().ascent;
            let _ = window.paint_glyph_run(point(x, y), run, self.theme.muted);
        }
    }
}

impl Render for FontView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (laid, painted) = (cx.entity(), cx.entity());
        canvas(
            move |bounds, window, cx| laid.update(cx, |view, _| view.layout(bounds, window)),
            move |bounds, (released, facts), window, cx| {
                for image in released {
                    let _ = window.drop_image(image);
                }
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    painted.read(cx).paint(bounds, facts.as_deref(), window)
                });
            },
        )
        .size_full()
    }
}
