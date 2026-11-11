//! The code view (ui/CodeLines.qml, PreviewText.qml, CodeRefusal.qml): the lines of one file or text coloured by the
//! backend, asked a screen and a margin at a time, painted as pre-shaped glyph runs with a muted number gutter.
pub mod bar;
mod capture;
pub mod paint;
mod scroll;
mod search_match;
mod selection;
mod selection_source;
pub mod shape;
#[cfg(test)]
mod vf4_tests;

pub use bar::{KindBar, ViewWords};
use flea::i18n::I18n;
use flea::ui_logic_doc_preview::code_view::{
    self, Answer, Lines, Reply, Window as View, BYTE_GATE,
};
use flea::ui_logic_doc_preview::refusal::Refusal;
use gpui::{Context, Font, Hsla, Pixels, SharedString};
pub use shape::RowCache;
use shape::{Lru, Prepared};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

/// The backend's `code` route; the token it answers comes back through `CodeView::reply`.
pub trait CodeHost {
    fn ask_code(&self, slot: &str, ask: &code_view::Ask) -> u64;
}

/// The face the lines are drawn in: the body size for code, the caption size for the gutter.
#[derive(Clone, Debug, PartialEq)]
pub struct Typeface {
    pub font: Font,
    pub body: Pixels,
    pub caption: Pixels,
}

/// The theme roles the view draws with, from the host's theme snapshot; the inks are already lifted to 4.5:1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub foreground: Hsla,
    pub muted: Hsla,
    pub muted_frame: Hsla,
    /// The line numbers' ink, the muted role lifted to 4.5:1 (ql-code CHANGES).
    pub gutter: Hsla,
    /// Indexed by the reply's role: the foreground, then red, green, yellow, blue, magenta and cyan.
    pub inks: [Hsla; 7],
}

/// The spacing tokens, from the host's scaled theme (board ql-code: 14 inset, rows 20, text 20 from the right edge).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub inset: f32,
    pub top: f32,
    pub right: f32,
    pub row: f32,
    pub gap: f32,
    pub state_mark: f32,
}

/// The kind's mark as an atlas key and its SVG, the row's own mark (Rust, HTML, a file).
pub type Mark = (SharedString, &'static [u8]);
/// The prepared lines, keyed by the text width and wrap they were prepared for.
type PreparedLines = (Option<(usize, bool)>, Lru<usize, Rc<Prepared>>);

pub struct CodeView {
    pub selection: crate::g2::doc_preview::selection::Selection,
    selection_original: Option<Option<crate::g2::doc_preview::selection::Lines>>,
    selection_read: Option<gpui::Task<()>>,
    pub lines: Lines,
    host: Option<Box<dyn CodeHost>>,
    pub slot: &'static str,
    /// The host's wish; the lines run only while it holds and the file is under the gate.
    pub active: bool,
    pub size: u64,
    pub max_bytes: u64,
    /// The caller's own read failed (the remote route).
    pub read_failed: bool,
    pub numbered: bool,
    pub wrap: bool,
    pub typeface: Typeface,
    pub palette: Palette,
    pub metrics: Metrics,
    pub mark: Mark,
    /// The first line in view, 0-based, and how far into it the view starts.
    pub top: usize,
    pub offset: f32,
    /// The viewport the lines were last laid out in, logical pixels.
    pub frame: (f32, f32),
    /// One advance of "0" at the body size, read once per face.
    pub(super) advance: Option<Pixels>,
    pub cache: RowCache,
    /// Each held line laid out once for the cells and wrap it was prepared at.
    prepared: RefCell<PreparedLines>,
    catalog: Option<Arc<I18n>>,
    /// Frames painted and lines shaped, for the count tests and the native idle check.
    pub painted: Cell<usize>,
    /// Rows painted and line wraps computed, for the count tests.
    pub painted_rows: Cell<usize>,
    painted_syntax: RefCell<Vec<capture::Syntax>>,
    pub wraps: Cell<usize>,
    /// Frames built by layout, for the count tests: an unchanged frame builds none.
    pub built: Cell<usize>,
    /// Bumped by every change a frame is built from; with the view's geometry it keys the last frame built.
    revision: u64,
    laid: Option<(paint::FrameKey, Rc<paint::Frame>)>,
    scroll: crate::g2::quick_look_overlay::scroller::Scroll,
    scroll_timer: Option<gpui::Task<()>>,
    // End remains anchored while its final window is still being read.
    end_pending: bool,
    matched: Option<search_match::Match>,
}

impl CodeView {
    pub fn new(
        slot: &'static str,
        typeface: Typeface,
        palette: Palette,
        metrics: Metrics,
        mark: Mark,
    ) -> Self {
        CodeView {
            selection: Default::default(),
            selection_original: None,
            selection_read: None,
            lines: Lines::new(1),
            host: None,
            slot,
            active: false,
            size: 0,
            max_bytes: BYTE_GATE,
            read_failed: false,
            numbered: true,
            wrap: false,
            typeface,
            palette,
            metrics,
            mark,
            top: 0,
            offset: 0.,
            frame: (0., 0.),
            advance: None,
            cache: RowCache::default(),
            prepared: RefCell::default(),
            catalog: None,
            painted: Cell::new(0),
            painted_syntax: RefCell::default(),
            painted_rows: Cell::new(0),
            wraps: Cell::new(0),
            built: Cell::new(0),
            revision: 0,
            laid: None,
            scroll: Default::default(),
            scroll_timer: None,
            end_pending: false,
            matched: None,
        }
    }

    /// The backend that colours the lines; None leaves the caller's own text drawn plain.
    pub fn set_host(&mut self, host: Option<Box<dyn CodeHost>>, cx: &mut Context<Self>) {
        self.selection.clear();
        self.selection_original = None;
        self.selection_read = None;
        self.lines.engine = host.is_some();
        self.host = host;
        self.lines.fresh();
        self.drop_shapes();
        self.revision += 1;
        cx.notify();
    }

    pub fn set_catalog(&mut self, catalog: Arc<I18n>, cx: &mut Context<Self>) {
        self.catalog = Some(catalog);
        self.revision += 1;
        cx.notify();
    }

    /// A new file: past the gate nothing is asked; the remote route hands the bytes it read (None while the read is out).
    pub fn set_subject(
        &mut self,
        path: &str,
        size: u64,
        from_text: bool,
        text: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.selection.clear();
        self.selection_original = None;
        self.selection_read = None;
        self.size = size;
        self.read_failed = false;
        self.top = 0;
        self.matched = None;
        self.offset = 0.;
        self.scroll = Default::default();
        self.scroll_timer = None;
        self.end_pending = false;
        self.drop_shapes();
        let gated = size > self.max_bytes;
        self.lines.active = self.active && !gated;
        self.lines
            .set_subject(path, from_text, if gated { None } else { text });
        self.flush(cx);
    }

    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if !active {
            self.end_pending = false;
            self.selection.clear();
            self.selection_read = None;
        }
        self.active = active;
        self.lines.set_active(active && self.size <= self.max_bytes);
        self.flush(cx);
    }

    /// The caller's read of a remote file failed.
    pub fn set_read_failed(&mut self, cx: &mut Context<Self>) {
        self.selection.clear();
        self.selection_read = None;
        self.read_failed = true;
        self.revision += 1;
        cx.notify();
    }

    /// Wrapping keeps the line at the top of the view at the top, since the view scrolls by line.
    pub fn set_wrap(&mut self, wrap: bool, cx: &mut Context<Self>) {
        if self.wrap != wrap {
            self.wrap = wrap;
            self.offset = 0.;
            self.drop_shapes();
            self.revision += 1;
            cx.notify();
        }
    }

    /// The lines in view as the core asks for them.
    pub fn view(&self) -> View {
        let rows = (self.frame.1 / self.metrics.row.max(1.)).ceil() as usize;
        View {
            top: self.top + 1,
            rows,
        }
    }

    /// Sends the ask the view needs, if any; called after every change that can move the window.
    pub fn flush(&mut self, cx: &mut Context<Self>) {
        self.revision += 1;
        if self.lines.due || self.lines.active {
            if let Some(ask) = self.lines.ask(self.view()) {
                if let Some(host) = &self.host {
                    let token = host.ask_code(self.slot, &ask);
                    self.lines.sent(token);
                }
            }
        }
        cx.notify();
    }

    /// One `code` reply from the host; any other token is dropped.
    pub fn reply(&mut self, reply: Reply, cx: &mut Context<Self>) {
        if self.lines.answered(reply) {
            self.settle_end();
            if self.refusal().is_some() {
                self.selection.clear();
            }
            self.flush(cx);
        }
    }

    pub fn refusal(&self) -> Option<Refusal> {
        code_view::refusal(
            self.size,
            self.max_bytes,
            self.lines.answer,
            self.read_failed,
        )
    }

    pub fn first_screen(&self) -> bool {
        self.refusal().is_some() || self.lines.first_screen(self.view())
    }

    /// "loading" until the answer, "ready" after it, or the refusal's status sentence.
    pub fn status(&self, i18n: &I18n) -> String {
        match self.refusal().and_then(Refusal::status_key) {
            Some(key) => i18n.tr(key),
            None if self.lines.answer != Answer::Pending || self.refusal().is_some() => {
                "ready".into()
            }
            None => "loading".into(),
        }
    }

    /// The sentence a failed read draws alone, which the host's error recipe draws (the PDF view's seam).
    pub fn fail_sentence(&self, i18n: &I18n) -> Option<String> {
        self.refusal()
            .and_then(Refusal::sentence_key)
            .map(|key| i18n.tr(key))
    }

    /// The line count for the bar once the lines are settled, as the host's chrome bar draws it.
    pub fn count_label(&self, i18n: &I18n) -> Option<String> {
        self.lines
            .answer
            .settled()
            .then(|| i18n.trn("document.line_count", self.lines.total as i64))
    }

    /// Quick Look's bar for a code file: the row's mark, the name, the count and the close mark, no view words.
    pub fn bar(&self, i18n: &I18n) -> KindBar {
        KindBar::new(
            self.mark.clone(),
            &self.lines.path,
            self.count_label(i18n),
            None,
            i18n,
        )
    }

    // Drops every shaped row and prepared line, for a new subject, host or wrap.
    fn drop_shapes(&mut self) {
        self.cache.clear();
        self.prepared.get_mut().1.clear();
    }

    /// A held line laid out at the view's cells, prepared once and then read from the cache; None for an unheld line.
    pub fn prepared(&self, line: usize) -> Option<Rc<Prepared>> {
        let advance = self.advance?;
        let runs = self.lines.row(line + 1)?;
        let key = (self.text_cells(advance), self.wrap);
        let mut prepared = self.prepared.borrow_mut();
        if prepared.0 != Some(key) {
            prepared.1.clear();
            prepared.0 = Some(key);
        }
        Some(prepared.1.get_or_make(line, || {
            self.wraps.set(self.wraps.get() + 1);
            Rc::new(shape::prepare(runs, key.0, key.1))
        }))
    }

    /// Bounds the prepared lines to the cache's reach around the view.
    pub(super) fn bound_prepared(&mut self, lines: usize) {
        self.prepared.get_mut().1.bound(lines);
    }

    /// The rows a line takes: one unless wrapped, then its wrap at the text's cells; an unheld line is one row.
    pub fn rows_of(&self, line: usize) -> usize {
        if !self.wrap {
            return 1;
        }
        self.prepared(line)
            .map_or(1, |prepared| prepared.rows.len())
    }

    /// The rows held shaped, for the count tests.
    pub fn shaped_rows(&self) -> usize {
        self.cache.len()
    }

    /// Whether the line's first row carries a gutter number, for the probes; an unshaped line carries none.
    pub fn has_number(&self, line: usize) -> bool {
        self.cache
            .rows
            .peek(&(line, 0))
            .is_some_and(|shaped| shaped.number.is_some())
    }

    /// The cells the text has between its start and the right inset.
    pub fn text_cells(&self, advance: Pixels) -> usize {
        let left = self.text_left(advance);
        ((self.frame.0 - left - self.metrics.right) / f32::from(advance))
            .floor()
            .max(1.) as usize
    }

    /// Where the gutter ends and where the text starts, logical pixels from the frame's left.
    pub fn gutter_right(&self, advance: Pixels) -> f32 {
        if !self.numbered {
            return self.metrics.inset;
        }
        let cells = flea::ui_logic_doc_preview::code_text::gutter_cells(self.lines.shown());
        self.metrics.inset + cells as f32 * f32::from(advance)
    }

    /// The two-cell lead separates numbered code from its gutter; plain text starts at the frame's inset.
    pub fn text_left(&self, advance: Pixels) -> f32 {
        let lead = if self.numbered {
            flea::ui_logic_doc_preview::code_text::TEXT_LEAD as f32 * f32::from(advance)
        } else {
            0.
        };
        (self.gutter_right(advance) + lead).round()
    }
}
