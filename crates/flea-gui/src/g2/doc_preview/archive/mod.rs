//! Quick Look's archive page (ui/Preview.qml archivePane): the name, the index's count line, as many entries as the
//! frame has room for, each with its file or folder mark, and the "+ N more" line; a failed index draws one sentence.
use super::code::shape::{self, Elide};
use super::code::Mark;
use flea::i18n::I18n;
use flea::ui_logic_doc_preview::archive::{self, Meta};
use flea::ui_logic_doc_preview::refusal::Refusal;
use gpui::{
    canvas, point, prelude::*, px, size, Bounds, ContentMask, Context, Font, GlyphRun, Hsla,
    Pixels, Render, TransformationMatrix, Window,
};
use std::{cell::Cell, rc::Rc, sync::Arc};

/// The `meta` route the archive fields come from; its answer comes back through `ArchiveView::answered`.
pub trait ArchiveHost {
    fn ask_meta(&self, path: &str);
}

/// The roles the page draws with (board ql-archive: the name in the foreground, the rest muted).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
}

/// The board's spacing (left and top 14, gap 9, rows 22) and its two text sizes.
#[derive(Clone, Debug, PartialEq)]
pub struct Metrics {
    pub inset: f32,
    pub gap: f32,
    pub row: f32,
    pub glyph: f32,
    pub font: Font,
    pub name: Pixels,
    pub caption: Pixels,
    pub more_opacity: f32,
}

/// One line of text as drawn: the text kept and its shaped run.
pub type Text = (String, Rc<GlyphRun>);

/// What one frame paints: the name, the count line, the listed rows and the "+ N more" line, each shaped once.
#[derive(Default)]
pub struct Frame {
    pub name: Option<Text>,
    pub line: Option<Text>,
    pub rows: Vec<(bool, Text)>,
    pub more: Option<Text>,
}

pub struct ArchiveView {
    pub path: String,
    pub active: bool,
    pub meta: Option<Meta>,
    host: Option<Box<dyn ArchiveHost>>,
    asked: Option<String>,
    pub palette: Palette,
    pub metrics: Metrics,
    pub mark: Mark,
    pub file_mark: Mark,
    pub folder_mark: Mark,
    catalog: Option<Arc<I18n>>,
    frame: Option<((usize, u32), Rc<Frame>)>,
    pub asks: usize,
    pub painted: Cell<usize>,
    pub shaped: usize,
    /// The preview column's Archive tile (PreviewArchive.qml): the entries alone, the name and count drawn under the frame.
    pub column: bool,
}

impl ArchiveView {
    pub fn new(
        palette: Palette,
        metrics: Metrics,
        mark: Mark,
        file_mark: Mark,
        folder_mark: Mark,
    ) -> Self {
        ArchiveView {
            path: String::new(),
            active: false,
            meta: None,
            host: None,
            asked: None,
            palette,
            metrics,
            mark,
            file_mark,
            folder_mark,
            catalog: None,
            frame: None,
            asks: 0,
            painted: Cell::new(0),
            shaped: 0,
            column: false,
        }
    }

    pub fn set_host(&mut self, host: Option<Box<dyn ArchiveHost>>) {
        self.host = host;
        self.asked = None;
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.catalog = Some(catalog);
        self.frame = None;
        cx.notify();
    }

    /// A new row: the old index goes at once, so another archive's names never show under this name.
    pub fn set_subject(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.path != path {
            self.path = path.to_string();
            self.meta = None;
            self.frame = None;
        }
        self.ask();
        cx.notify();
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        self.active = active;
        self.ask();
        cx.notify();
    }

    // One ask per path while active; the index is read by the backend, never here.
    fn ask(&mut self) {
        if !self.active
            || self.path.is_empty()
            || self.meta.is_some()
            || self.asked.as_deref() == Some(&self.path)
        {
            return;
        }
        if let Some(host) = &self.host {
            host.ask_meta(&self.path);
            self.asked = Some(self.path.clone());
            self.asks += 1;
        }
    }

    /// The `meta` answer for a path; one for another row is dropped.
    pub fn answered(&mut self, path: &str, meta: Meta, cx: &mut Context<Self>) -> bool {
        if path != self.path {
            return false;
        }
        self.meta = Some(meta);
        self.frame = None;
        cx.notify();
        true
    }

    pub fn refusal(&self) -> Option<Refusal> {
        archive::refusal(self.meta.as_ref())
    }

    /// "loading" until the index lands, "archive" after, or the unreadable sentence.
    pub fn status(&self, i18n: &I18n) -> String {
        match self.refusal().and_then(Refusal::status_key) {
            Some(key) => i18n.tr(key),
            None if self.meta.is_some() => "archive".into(),
            None => "loading".into(),
        }
    }

    pub fn fail_sentence(&self, i18n: &I18n) -> Option<String> {
        self.refusal()
            .and_then(Refusal::sentence_key)
            .map(|key| i18n.tr(key))
    }

    /// The rows the frame lists below the name and the count line, the "+ N more" line held back.
    pub fn shown(&self, height: f32) -> usize {
        let m = &self.metrics;
        let head = if self.column {
            2. * m.inset
        } else {
            m.inset + f32::from(m.name) * 1.3 + m.gap + f32::from(m.caption) * 1.3 + m.gap
        };
        archive::shown(f64::from((height - head).max(0.)), f64::from(m.row))
    }

    /// The texts the frame draws for a height, as the probes read them.
    pub fn texts(&self, i18n: &I18n, height: f32) -> Option<(String, Vec<String>, Option<String>)> {
        let meta = self.meta.as_ref().filter(|meta| !meta.failed)?;
        let shown = self.shown(height);
        let rows = archive::entries(meta, shown)
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        Some((
            archive::line(i18n, meta),
            rows,
            archive::more_line(i18n, meta, shown),
        ))
    }

    /// The texts the last frame drew and their widths, for the probes.
    pub fn drawn(&self) -> Vec<(String, f32)> {
        let Some((_, frame)) = &self.frame else {
            return Vec::new();
        };
        let rows = frame.rows.iter().map(|(_, text)| text);
        let all = frame
            .name
            .iter()
            .chain(frame.line.iter())
            .chain(rows)
            .chain(frame.more.iter());
        all.map(|(text, run)| (text.clone(), f32::from(run.line().width)))
            .collect()
    }

    fn layout(&mut self, bounds: Bounds<Pixels>, window: &Window) -> Option<Rc<Frame>> {
        let height = f32::from(bounds.size.height);
        let width = f32::from(bounds.size.width);
        let key = (self.shown(height), width.to_bits());
        if let Some((_, frame)) = self.frame.as_ref().filter(|(at, _)| *at == key) {
            return Some(Rc::clone(frame));
        }
        let catalog = self.catalog.clone()?;
        let meta = self.meta.as_ref().filter(|meta| !meta.failed)?;
        let m = self.metrics.clone();
        let full = px(width - 2. * m.inset);
        let text = |text: String, size: Pixels, room: Pixels, elide: Elide| {
            let (kept, run) = shape::elided(window, &text, &m.font, size, room, elide);
            (kept, Rc::new(run))
        };
        let beside = full - px(m.glyph + m.gap);
        let name = std::path::Path::new(&self.path)
            .file_name()
            .map_or(self.path.clone(), |n| n.to_string_lossy().into());
        let shown = key.0;
        let head = !self.column;
        let frame = Rc::new(Frame {
            name: head.then(|| text(name, m.name, full, Elide::Middle)),
            line: head.then(|| text(archive::line(&catalog, meta), m.caption, full, Elide::End)),
            rows: archive::entries(meta, shown)
                .iter()
                .map(|(entry, dir)| (*dir, text(entry.clone(), m.caption, beside, Elide::End)))
                .collect(),
            more: archive::more_line(&catalog, meta, shown)
                .map(|more| text(more, m.caption, full, Elide::End)),
        });
        self.shaped += 2 * usize::from(head) + frame.rows.len() + usize::from(frame.more.is_some());
        self.frame = Some((key, Rc::clone(&frame)));
        Some(frame)
    }

    fn paint(
        &self,
        bounds: Bounds<Pixels>,
        frame: Option<&Frame>,
        window: &mut Window,
        cx: &gpui::App,
    ) {
        self.painted.set(self.painted.get() + 1);
        let Some(frame) = frame else { return };
        let (m, p) = (&self.metrics, self.palette);
        let left = bounds.origin.x + px(m.inset);
        let mut y = bounds.origin.y + px(m.inset);
        let text = |run: &GlyphRun,
                    x: Pixels,
                    top: Pixels,
                    height: Pixels,
                    ink: Hsla,
                    window: &mut Window| {
            let line = run.line();
            let baseline =
                window.pixel_snap(top + (height - line.ascent - line.descent) / 2. + line.ascent);
            let _ = window.paint_glyph_run(point(x, baseline), run, ink);
        };
        if let Some((_, name)) = &frame.name {
            let height = m.name * 1.3;
            text(name, left, y, height, p.foreground, window);
            y += height + px(m.gap);
        }
        if let Some((_, line)) = &frame.line {
            let height = m.caption * 1.3;
            text(line, left, y, height, p.muted, window);
            y += height + px(m.gap);
        }
        let row = px(m.row);
        let glyph = px(m.glyph);
        for (dir, (_, run)) in &frame.rows {
            let (key, data) = if *dir {
                &self.folder_mark
            } else {
                &self.file_mark
            };
            let mark = Bounds::new(point(left, y + (row - glyph) / 2.), size(glyph, glyph));
            let _ = window.paint_svg(
                mark,
                key.clone(),
                Some(*data),
                TransformationMatrix::unit(),
                p.muted_frame,
                cx,
            );
            text(run, left + glyph + px(m.gap), y, row, p.muted, window);
            y += row;
        }
        if let Some((_, more)) = &frame.more {
            text(more, left, y, row, p.muted.opacity(m.more_opacity), window);
        }
    }
}

impl Render for ArchiveView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (laid, painted) = (cx.entity(), cx.entity());
        canvas(
            move |bounds, window, cx| laid.update(cx, |view, _| view.layout(bounds, window)),
            move |bounds, frame, window, cx| {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    painted.read(cx).paint(bounds, frame.as_deref(), window, cx)
                })
            },
        )
        .size_full()
    }
}
