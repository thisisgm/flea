//! The HTML preview (ui/PreviewHtml.qml, HtmlPane.qml, HtmlClip.qml): the helper's still image of the page under the
//! dark-theme veil and clipped to the frame's inner arc, or the shipped code view for Source.
pub(crate) mod capture;
pub mod decode;
mod paint;
mod scroll;

use super::code::{self, CodeView, KindBar, Mark};
use super::Palette;
use flea::i18n::I18n;
use flea::ui_logic_doc_preview::code_view::{Answer, Lines, BYTE_GATE, REMOTE_GATE};
use flea::ui_logic_doc_preview::html::{Command, Html, Picture, Reply, View};
use flea::ui_logic_doc_preview::refusal::Refusal;
use gpui::{App, AppContext as _, Context, Entity, GlyphRun, RenderImage, Task};
use std::{cell::Cell, rc::Rc, sync::Arc, time::Instant};

/// The backend's `html` route; the token it answers comes back through `HtmlView::reply`.
pub trait HtmlHost {
    fn ask_html(&self, slot: &str, path: &str, width: u32, height: u32) -> u64;
    fn cancel_html(&self, slot: &str, width: u32);
}

/// The frame's tokens: one scroll step, the frame's inner radius and whether the top corners round (the column's do).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub row_height: f32,
    pub radius: f32,
    pub round_top: bool,
}

/// Everything Source's code view is built from, kept until Source first shows.
pub struct SourceParts {
    pub typeface: code::Typeface,
    pub palette: code::Palette,
    pub metrics: code::Metrics,
}

pub struct HtmlView {
    pub doc: Html,
    host: Option<Box<dyn HtmlHost>>,
    code_host: Option<Box<dyn Fn() -> Box<dyn code::CodeHost>>>,
    pub source: Option<Entity<CodeView>>,
    parts: SourceParts,
    pub palette: Palette,
    pub metrics: Metrics,
    pub mark: Mark,
    pub wrap: bool,
    /// The column's compact frame draws no refusal frame of its own; the column stands the kind's mark in.
    pub compact: bool,
    catalog: Option<Arc<I18n>>,
    /// The decoded page and the token it was asked under; the old page stays up until a new one decodes.
    pub image: Option<(u64, Arc<RenderImage>, (u32, u32))>,
    released: Vec<Arc<RenderImage>>,
    bars: crate::g2::quick_look_overlay::scroller::overlay::Overlay,
    pub scroll_y: f32,
    pub scroll_x: f32,
    /// Network or phone storage, set with the path: Source draws the bytes the caller read, never sends the path.
    pub remote: bool,
    /// The generation of the page's current `set_path`; a read quoting an older one is superseded.
    read_gen: u64,
    source_text: Option<String>,
    source_read_failed: bool,
    pub frame: (f32, f32),
    /// Where the page was last painted, for htmlState.
    pub bounds: Option<gpui::Bounds<gpui::Pixels>>,
    pub(crate) frame_style: Cell<Option<capture::FrameStyle>>,
    pub(crate) frame_bounds: Cell<Option<gpui::Bounds<gpui::Pixels>>>,
    origin: Instant,
    timer: Option<(u64, Task<()>)>,
    decode: Option<Task<()>>,
    /// Frames painted and pages decoded, for the count tests and the native idle check.
    pub painted: Cell<usize>,
    pub decoded: usize,
    /// Command flushes, for the count tests: an unchanged frame flushes nothing.
    pub flushes: usize,
    /// The frame and scale last laid out, and the too-large caption with the catalog it was shaped for and its scale.
    laid: Option<(f32, f32, f32)>,
    caption: Option<(Arc<I18n>, u32, Rc<GlyphRun>)>,
}

impl HtmlView {
    pub fn new(
        slot: &str,
        palette: Palette,
        metrics: Metrics,
        parts: SourceParts,
        mark: Mark,
        cx: &mut Context<Self>,
    ) -> Self {
        HtmlView {
            doc: Html::new(slot),
            host: None,
            code_host: None,
            source: None,
            parts,
            palette,
            metrics,
            mark,
            wrap: false,
            compact: slot != "look",
            catalog: None,
            image: None,
            released: Vec::new(),
            bars: Default::default(),
            scroll_y: 0.,
            scroll_x: 0.,
            remote: false,
            read_gen: 0,
            source_text: None,
            source_read_failed: false,
            frame: (0., 0.),
            bounds: None,
            frame_style: Cell::new(None),
            frame_bounds: Cell::new(None),
            origin: cx.background_executor().now(),
            timer: None,
            decode: None,
            painted: Cell::new(0),
            decoded: 0,
            flushes: 0,
            laid: None,
            caption: None,
        }
    }

    pub fn now(&self, cx: &App) -> u64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
            .as_millis() as u64
    }

    /// The backend's html route, and the factory for Source's code route; None reads as unreadable on the first ask.
    pub fn set_hosts(
        &mut self,
        host: Option<Box<dyn HtmlHost>>,
        code_host: Option<Box<dyn Fn() -> Box<dyn code::CodeHost>>>,
    ) {
        self.host = host;
        self.code_host = code_host;
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        if let Some(source) = &self.source {
            let catalog = Arc::clone(&catalog);
            source.update(cx, |code, cx| code.set_catalog(catalog, cx));
        }
        self.catalog = Some(catalog);
        cx.notify();
    }

    /// The page and its storage class, set together: network or phone storage (PreviewHtml.qml truncate) takes the
    /// remote gate, and Source then draws only the caller's own read of this page, never the path or another page's bytes.
    /// Returns the read generation the caller quotes to `set_source_text`; each call starts a new generation.
    pub fn set_path(&mut self, path: &str, size: u64, remote: bool, cx: &mut Context<Self>) -> u64 {
        let now = self.now(cx);
        self.read_gen += 1;
        self.remote = remote;
        self.source_text = None;
        self.source_read_failed = false;
        self.doc.max_bytes = if remote { REMOTE_GATE } else { BYTE_GATE };
        self.doc.set_path(path, size, now);
        self.scroll_y = 0.;
        self.bars = Default::default();
        if let Some(old) = self.image.take() {
            self.released.push(old.1);
        }
        self.decode = None;
        self.sync_source(true, cx);
        self.flush(cx);
        self.read_gen
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        let now = self.now(cx);
        self.doc.set_active(active, now);
        self.sync_source(false, cx);
        self.flush(cx);
    }

    /// R flips the view in this Quick Look only; Source builds its code view the first time it shows.
    pub fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        let now = self.now(cx);
        self.doc.set_view(view, now);
        self.sync_source(false, cx);
        self.flush(cx);
    }

    pub fn set_wrap(&mut self, wrap: bool, cx: &mut Context<Self>) {
        self.wrap = wrap;
        if let Some(source) = &self.source {
            source.update(cx, |code, cx| code.set_wrap(wrap, cx));
        }
    }

    // Source's code view follows the subject and runs only while Source shows and the page is under the gate.
    fn sync_source(&mut self, subject: bool, cx: &mut Context<Self>) {
        if self.doc.source_built && self.source.is_none() {
            let parts = &self.parts;
            let (typeface, palette, metrics, mark) = (
                parts.typeface.clone(),
                parts.palette,
                parts.metrics,
                self.mark.clone(),
            );
            let host = self.code_host.as_ref().map(|make| make());
            let (wrap, catalog) = (self.wrap, self.catalog.clone());
            self.source = Some(cx.new(|cx| {
                let mut view = CodeView::new("look", typeface, palette, metrics, mark);
                view.wrap = wrap;
                view.set_host(host, cx);
                if let Some(catalog) = catalog {
                    view.set_catalog(catalog, cx);
                }
                view
            }));
            self.sync_source(true, cx);
            return;
        }
        let Some(source) = &self.source else { return };
        let active = self.doc.active && self.doc.source_view() && !self.doc.over_size();
        let (path, size, remote) = (self.doc.path.clone(), self.doc.size, self.remote);
        let (text, failed) = (
            self.source_text.clone().filter(|_| remote),
            remote && self.source_read_failed,
        );
        source.update(cx, |code, cx| {
            if subject {
                code.max_bytes = if remote { REMOTE_GATE } else { BYTE_GATE };
                code.set_subject(&path, size, remote, text, cx);
                if failed {
                    code.set_read_failed(cx);
                }
            }
            code.set_active(active, cx);
        });
    }

    fn source_lines<R>(&self, cx: &App, read: impl FnOnce(&Lines, Option<&CodeView>) -> R) -> R {
        match &self.source {
            Some(source) => {
                let code = source.read(cx);
                read(&code.lines, Some(code))
            }
            None => read(&Lines::default(), None),
        }
    }

    fn source_answer(&self, cx: &App) -> (Answer, bool) {
        self.source_lines(cx, |lines, code| {
            (lines.answer, code.is_some_and(CodeView::first_screen))
        })
    }

    /// Source reads its own code view's refusal, which carries the remote gate and the caller's failed read.
    pub fn refusal(&self, cx: &App) -> Option<Refusal> {
        match (&self.source, self.doc.source_view()) {
            (Some(source), true) => source.read(cx).refusal(),
            _ => self.doc.refusal(self.source_answer(cx).0),
        }
    }

    pub fn first_screen(&self, cx: &App) -> bool {
        let (answer, first) = self.source_answer(cx);
        self.doc.first_screen(answer, first)
    }

    /// pv1 draws its loading mark while this holds; a re-ask of a drawn page never shows it.
    pub fn loading(&self, cx: &App) -> bool {
        let (answer, first) = self.source_answer(cx);
        self.doc.loading(answer, first)
    }

    pub fn content_ready(&self, cx: &App) -> bool {
        let settled = self.source_lines(cx, |lines, _| lines.answer.settled());
        self.doc.content_ready(settled)
    }

    /// "loading", "ready" or the refusal's status sentence (PreviewHtml.qml status).
    pub fn status(&self, i18n: &I18n, cx: &App) -> String {
        match self.refusal(cx) {
            Some(refusal) => refusal
                .status_key()
                .map_or_else(|| "ready".into(), |key| i18n.tr(key)),
            None if self.first_screen(cx) => "ready".into(),
            None => "loading".into(),
        }
    }

    /// The sentence a failed page draws alone, which the host's error recipe draws.
    pub fn fail_sentence(&self, i18n: &I18n, cx: &App) -> Option<String> {
        self.refusal(cx)
            .and_then(Refusal::sentence_key)
            .map(|key| i18n.tr(key))
    }

    /// The whole-file line count: Source's own once settled, else the helper's.
    pub fn line_count(&self, cx: &App) -> usize {
        self.source_lines(cx, |lines, _| {
            if self.doc.source_view() && lines.answer.settled() {
                lines.total
            } else {
                self.doc.total
            }
        })
    }

    /// Quick Look's bar: the HTML mark, the name, the count once the content is ready, Rendered and Source, close.
    pub fn bar(&self, i18n: &I18n, cx: &App) -> KindBar {
        let count = self
            .content_ready(cx)
            .then(|| i18n.trn("document.line_count", self.line_count(cx) as i64));
        KindBar::new(
            self.mark.clone(),
            &self.doc.path,
            count,
            Some(self.doc.view),
            i18n,
        )
    }

    // Sends what the document asked for and arms the settle's one wake.
    fn flush(&mut self, cx: &mut Context<Self>) {
        self.flushes += 1;
        for command in self.doc.take_commands() {
            match (command, &self.host) {
                (
                    Command::Ask {
                        slot,
                        path,
                        width,
                        height,
                    },
                    Some(host),
                ) => {
                    let token = host.ask_html(&slot, &path, width, height);
                    self.doc.sent(token);
                }
                (Command::Ask { .. }, None) => self.doc.no_backend(),
                (Command::Cancel { slot, width }, Some(host)) => host.cancel_html(&slot, width),
                (Command::Cancel { .. }, None) => {}
            }
        }
        self.schedule(cx);
        cx.notify();
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        let Some(due) = self.doc.deadline() else {
            self.timer = None;
            return;
        };
        if matches!(self.timer, Some((at, _)) if at == due) {
            return;
        }
        let wait = due.saturating_sub(self.now(cx));
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(wait))
                .await;
            let _ = this.update(cx, |view, cx| {
                view.timer = None;
                let now = view.now(cx);
                view.doc.tick(now);
                view.flush(cx);
            });
        });
        self.timer = Some((due, task));
    }

    /// The file changed on disk: one ask at once, the drawn page staying up with no loading mark until the new one lands.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.doc.ask();
        self.flush(cx);
    }

    /// One `html` reply; a matching ready reply decodes its PNG off the UI thread, and the old page stays up until it lands.
    pub fn reply(&mut self, reply: Reply, cx: &mut Context<Self>) {
        if !self.doc.reply(&reply) {
            return;
        }
        if self.doc.picture == Picture::Loading {
            let (token, file) = (self.doc.image_token, self.doc.image_file.clone());
            let job = cx
                .background_executor()
                .spawn(async move { decode::png(&file) });
            self.decode = Some(cx.spawn(async move |this, cx| {
                let decoded = job.await;
                let _ = this.update(cx, |view, cx| view.decoded(token, decoded, cx));
            }));
        }
        cx.notify();
    }
}
