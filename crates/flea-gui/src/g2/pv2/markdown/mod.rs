//! Markdown in the preview: the column's document and Quick Look's, rendered or as its source, the bar's data, and a first screen
//! that holds its first table, picture and figure; parsing, layout and decoding stay off the UI thread.
mod asks;
mod assets;
mod doc;
pub mod figures;
mod loading;
mod paint;
mod source;
pub mod style;
mod worker;

pub use asks::FIGURE_HOLD_MS;
pub use assets::{Art, Snapshot};
pub use doc::{Doc, Status};
pub use figures::{FigureDone, FigureHost};
pub use flea::ui_logic_pv2::markdown::markdown_keys::{Effect, View};
pub use source::Answer;
pub use style::{Faces, Palette};
pub use worker::{line_starts, TextMeasure, MAX_BYTES};

use flea::i18n::I18n;
use flea::ui_logic_pv2::markdown::{markdown_keys, markdown_layout::Metrics, NESTING_LIMIT};
use gpui::{Context, Entity, Subscription, Task, Window};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

/// The host's scaled theme tokens a document lays out with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// The document's body: the theme's body in Quick Look, its small body in the column.
    pub body: f32,
    pub theme_body: f32,
    pub caption: f32,
    pub gap: f32,
    pub hairline: f32,
    pub row_pad_x: f32,
    pub row_pad_y: f32,
    /// The chrome mark's size.
    pub mark: f32,
    /// The state mark's size, which a refused Source draws.
    pub state_mark: f32,
    /// True for the preview column's document; Quick Look's is false.
    pub column: bool,
}

impl Tokens {
    pub fn metrics(&self) -> Metrics {
        Metrics {
            body: self.body,
            theme_body: self.theme_body,
            caption: self.caption,
            gap: self.gap,
            hairline: self.hairline,
            row_pad_x: self.row_pad_x,
            mark: self.mark,
            column: self.column,
        }
    }

    /// The document's inset across: the row padding and the control padding, less the frame's hairline.
    pub fn inset_x(&self) -> f32 {
        self.row_pad_x + self.row_pad_y - self.hairline
    }

    pub fn inset_y(&self) -> f32 {
        self.gap + self.row_pad_y
    }
}

/// What a code ask names: the file when the backend may read it, or text the view holds with its name and language.
#[derive(Clone, Debug, PartialEq)]
pub enum CodeSource {
    Path(String),
    Text {
        text: Arc<str>,
        name: String,
        lang: String,
    },
}

/// The backend's code engine, `code` on the protocol; its reply comes back through `MarkdownView::code_answered`.
pub trait CodeHost {
    fn ask_code(&self, slot: &str, source: CodeSource, first: usize, count: usize) -> u64;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodeState {
    Styled,
    Plain,
    Binary,
    Large,
    Failed,
}

/// One code reply: the lines from `first`, each its runs of role and text.
#[derive(Clone, Debug, PartialEq)]
pub struct CodeAnswer {
    pub token: u64,
    pub state: CodeState,
    pub total: usize,
    pub first: usize,
    pub lines: Vec<Vec<(i64, String)>>,
}

/// The most lines one code ask names; a longer fence or file is asked in parts.
pub const CODE_LINE_CAP: usize = 400;

/// Quick Look's Markdown bar as the host's chrome bar draws it, every string already through the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub name: String,
    /// "12 lines", only once the file is read and parsed.
    pub count: Option<String>,
    /// Rendered and Source, each with its label and whether it is the view in effect.
    pub views: [(View, String, bool); 2],
    pub close: String,
    pub close_focused: bool,
}

/// The sentences the view writes, from the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct Words {
    pub remote: Arc<str>,
    pub deep: String,
    pub too_large: String,
    pub unreadable: String,
    pub binary: String,
    pub large: String,
}

impl Words {
    pub fn of(i18n: &I18n) -> Self {
        Words {
            remote: i18n.tr("document.markdown_remote_image").into(),
            deep: i18n.tr_args("document.markdown_too_deep", &[&NESTING_LIMIT.to_string()]),
            too_large: i18n.tr("document.markdown_too_large"),
            unreadable: i18n.tr("document.markdown_unreadable"),
            binary: i18n.tr("document.markdown_binary"),
            large: i18n.tr("document.markdown_large"),
        }
    }
}

/// Counts the suites read in place of a clock: what was parsed where, laid, shaped and drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counts {
    /// Bytes of a document parsed on the UI thread, which stays 0.
    pub ui_parse_bytes: usize,
    pub parsed_bytes: usize,
    pub parses: usize,
    pub layout_jobs: usize,
    /// Blocks in the first layout job and in the largest one.
    pub first_job: usize,
    pub largest_job: usize,
    pub blocks_laid: usize,
    pub frames: usize,
    /// Frames painted while the first screen was held, drawing no document.
    pub held_frames: usize,
    pub shaped_frame: usize,
    pub shaped_max: usize,
    pub items_frame: usize,
    pub items_max: usize,
    pub first: Option<FirstFrame>,
}

/// What the first frame that drew the shown document held; a new document starts it over.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FirstFrame {
    pub texts: usize,
    pub tables: usize,
    pub pictures: usize,
    /// Figures drawn at their size, neither asked nor failed.
    pub figures: usize,
    pub fences: usize,
}

pub struct MarkdownView {
    pub palette: Palette,
    pub tokens: Tokens,
    pub faces: Faces,
    /// The backend ask slot: "look" in Quick Look, "column" in the preview column.
    pub slot: String,
    pub words: Words,
    pub view: View,
    pub wrap: bool,
    pub close_focused: bool,
    pub active: bool,
    pub path: Option<PathBuf>,
    pub doc: Doc,
    pub source: source::Source,
    pub counts: Counts,
    /// The rendered document's scroll from its top, logical pixels.
    pub scroll: f32,
    /// The frame the document was last laid out in, and its scale.
    pub frame: (f32, f32),
    preparing: bool,
    confirming: bool,
    read_limit: u64,
    pub scale: f32,
    workers: worker::Workers,
    decoder: assets::Decoder,
    figures: Entity<FigureHost>,
    code: Option<Box<dyn CodeHost>>,
    glyphs: paint::Glyphs,
    ops: Vec<paint::Op>,
    hits: Vec<paint::Hit>,
    tables: HashMap<paint::TableId, f32>,
    pointer: paint::Pointer,
    /// The view's top left in the window, for pointer events.
    bounds: (f32, f32),
    hold_timer: Option<Task<()>>,
    origin: std::time::Instant,
    _tasks: Vec<Task<()>>,
    _figures: Subscription,
}

impl MarkdownView {
    pub fn new(
        palette: Palette,
        tokens: Tokens,
        faces: Faces,
        slot: &str,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Self {
        let workers = worker::Workers::spawn(cx.text_system().clone(), faces.clone());
        let decoder = assets::Decoder::spawn(cx.svg_renderer());
        let (replies, decoded) = (workers.replies.clone(), decoder.decoded.clone());
        let parse = cx.spawn(async move |this, cx| {
            while let Ok(reply) = replies.recv().await {
                if this.update(cx, |view, cx| view.reply(reply, cx)).is_err() {
                    break;
                }
            }
        });
        let pictures = cx.spawn(async move |this, cx| {
            while let Ok(done) = decoded.recv().await {
                if this.update(cx, |view, cx| view.decoded(done, cx)).is_err() {
                    break;
                }
            }
        });
        let figures = figures::host(cx);
        let subscription = cx.subscribe(&figures, |view, _, done: &FigureDone, cx| {
            view.figure_done(done, cx)
        });
        let measure = TextMeasure::new(cx.text_system().clone(), faces.clone());
        MarkdownView {
            palette,
            tokens,
            faces,
            slot: slot.to_string(),
            words: Words::of(i18n),
            view: View::Rendered,
            wrap: false,
            close_focused: false,
            active: false,
            path: None,
            doc: Doc::default(),
            source: source::Source::new(measure),
            counts: Counts::default(),
            scroll: 0.,
            frame: (0., 0.),
            preparing: false,
            confirming: false,
            read_limit: MAX_BYTES,
            scale: 1.,
            workers,
            decoder,
            figures,
            code: None,
            glyphs: paint::Glyphs::default(),
            ops: Vec::new(),
            hits: Vec::new(),
            tables: HashMap::new(),
            pointer: paint::Pointer::default(),
            bounds: (0., 0.),
            hold_timer: None,
            origin: cx.background_executor().now(),
            _tasks: vec![parse, pictures],
            _figures: subscription,
        }
    }

    /// Milliseconds on the app's clock, which a test advances by hand.
    pub fn now(&self, cx: &gpui::App) -> u64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
            .as_millis() as u64
    }

    /// The backend that colours fences and Source; without one both draw in the foreground.
    pub fn set_code_host(&mut self, host: Box<dyn CodeHost>, cx: &mut Context<Self>) {
        self.code = Some(host);
        self.doc.fences.clear();
        self.source.reset_asks();
        self.pump(cx);
    }

    /// The view the body draws: Source for a document nested past the limit, whatever was asked.
    pub fn shown_view(&self) -> View {
        if self.doc.deep() {
            View::Source
        } else {
            self.view
        }
    }

    pub fn set_view(&mut self, view: View, cx: &mut Context<Self>) {
        if self.view != view {
            self.view = view;
            self.pump(cx);
            cx.notify();
        }
    }

    /// One key action from the host's key table.
    pub fn action(&mut self, name: &str, cx: &mut Context<Self>) -> Effect {
        let effect = markdown_keys::action(name, self.view, self.close_focused);
        match effect {
            Effect::View(view) => self.set_view(view, cx),
            Effect::Wrap => {
                self.wrap = !self.wrap;
                cx.notify();
            }
            Effect::CloseFocus(focused) => {
                self.close_focused = focused;
                cx.notify();
            }
            Effect::Scroll(rows) => self.scroll_rows(rows, cx),
            Effect::Close | Effect::None => {}
        }
        effect
    }

    /// The bar's data; the host's chrome bar draws it.
    pub fn bar(&self, i18n: &I18n) -> Bar {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let shown = self.shown_view();
        Bar {
            name,
            count: self
                .doc
                .content_ready()
                .then(|| i18n.trn("document.markdown_lines", self.doc.lines as i64)),
            views: [
                (
                    View::Rendered,
                    i18n.tr("document.markdown_rendered"),
                    shown == View::Rendered,
                ),
                (
                    View::Source,
                    i18n.tr("document.markdown_source"),
                    shown == View::Source,
                ),
            ],
            close: i18n.tr("document.markdown_close"),
            close_focused: self.close_focused,
        }
    }

    /// The sentence the pane shows in the document's place, None while it loads or draws.
    pub fn status_sentence(&self) -> Option<&str> {
        match self.doc.status {
            Status::TooLarge => Some(&self.words.too_large),
            Status::Unreadable => Some(&self.words.unreadable),
            _ if self.shown_view() == View::Source => self.source.refusal(&self.words),
            _ => None,
        }
    }

    /// True while the document is read, parsed or held for its first screen; the host draws its loading mark meanwhile.
    pub fn loading(&self, cx: &gpui::App) -> bool {
        self.active
            && self.path.is_some()
            && self.status_sentence().is_none()
            && self.source_refused().is_none()
            && self.holding(self.now(cx))
    }

    /// The window's scale for pictures and figures drawn from now.
    pub fn set_scale(&mut self, window: &Window) {
        self.scale = window.scale_factor();
    }
}
