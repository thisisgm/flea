//! One retained selection preview per pane; completed frames survive replacement work.
use crate::{
    columns::preview_host::{PreviewProducer, ThirdSwap},
    g2::{
        doc_preview::PdfView, media_preview::player::MediaPlayer, model_preview::view::Model3dView,
    },
};
use flea::{
    jsondoc::Json,
    protocol::RowFacts,
    ui_logic_preview_core::{facts, kinds, kinds::State, meta::Meta},
};
use gpui::{Context, Entity, RenderImage, Subscription, Task};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

#[path = "selection_preview_doc_readers.rs"]
mod doc_readers;
#[path = "selection_preview_docs.rs"]
pub(crate) mod docs;
#[path = "selection_preview_fitted.rs"]
mod fitted;
#[path = "selection_preview_frames.rs"]
mod frame_feedback;
#[path = "selection_preview_host.rs"]
pub(crate) mod host;
#[path = "selection_preview_lifetime.rs"]
mod lifetime;
#[path = "selection_preview_load.rs"]
mod load;
#[path = "selection_preview_loading.rs"]
mod loading;
#[path = "selection_preview_multi.rs"]
pub(crate) mod multi;
#[path = "selection_preview_observation.rs"]
pub(crate) mod observation;
#[path = "selection_preview_owner.rs"]
mod owner;
#[path = "selection_preview_picture.rs"]
pub(crate) mod picture;
#[path = "selection_preview_play.rs"]
mod play;
#[path = "selection_preview_plugin.rs"]
pub(crate) mod plugin;
#[cfg(test)]
#[path = "selection_preview_plugin_tests.rs"]
mod plugin_tests;
#[path = "selection_preview_quality.rs"]
mod quality;
#[path = "selection_preview_readers.rs"]
pub(crate) mod readers;
#[path = "selection_preview_render.rs"]
pub(crate) mod render;
#[path = "selection_preview_text.rs"]
mod text;
#[path = "selection_preview_thumbnail_observer.rs"]
pub(crate) mod thumbnail_observer;
#[path = "selection_preview_wire.rs"]
pub(crate) mod wire;

static NEXT: AtomicU64 = AtomicU64::new(1);
fn token() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// What the frame draws for a row: the state Facts.js names it, with the document kinds the column mounts a view for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Text,
    Code,
    Markdown,
    Table,
    Html,
    Pdf,
    Video,
    Audio,
    Model,
    Archive,
    Office,
    Symlink,
    Facts,
    Multi,
}
impl Kind {
    /// The state ui/Ipc.qml previewColumnState reads: a document row keeps its row state's word.
    pub fn name(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Text | Self::Markdown | Self::Table => "text",
            Self::Code | Self::Html => "code",
            Self::Pdf => "pdf",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Model => "model3d",
            Self::Archive => "archive",
            Self::Office => "office",
            Self::Symlink => "symlink",
            Self::Facts => "unsupported",
            Self::Multi => "multi",
        }
    }
    /// Facts.js tall(): text, code, an archive list and a PDF fill the column down to the name.
    pub(crate) fn tall(self) -> bool {
        matches!(
            self,
            Self::Text
                | Self::Code
                | Self::Markdown
                | Self::Table
                | Self::Html
                | Self::Archive
                | Self::Pdf
        )
    }
    /// PreviewColumn.qml wantsThumb: the states whose frame draws a thumbnail when one exists.
    pub(crate) fn thumbed(self) -> bool {
        matches!(
            self,
            Self::Image | Self::Video | Self::Audio | Self::Symlink | Self::Facts
        )
    }
    /// The row's Facts.js state, then the document view its name picks.
    pub(crate) fn state(self) -> State {
        match self {
            Self::Image => State::Image,
            Self::Text | Self::Markdown | Self::Table => State::Text,
            Self::Code | Self::Html => State::Code,
            Self::Pdf => State::Pdf,
            Self::Video => State::Video,
            Self::Audio => State::Audio,
            Self::Archive => State::Archive,
            Self::Symlink => State::Symlink,
            Self::Multi => State::Multi,
            Self::Model | Self::Office | Self::Facts => State::Unsupported,
        }
    }
    fn of(row: &RowFacts) -> Self {
        let state = facts::state(Some(&fact_row(row)), 1, false, "", &row.kind);
        let name = row.name.to_ascii_lowercase();
        let ext = name.rsplit('.').next().unwrap_or("");
        if state == State::Symlink {
            return Self::Symlink;
        }
        if matches!(
            ext,
            "stl" | "3mf" | "obj" | "ply" | "amf" | "gcode" | "gco" | "bgcode"
        ) {
            return Self::Model;
        }
        if flea::ui_logic_doc_preview::office::Group::of(&name).is_some() {
            return Self::Office;
        }
        match state {
            State::Image => Self::Image,
            State::Video => Self::Video,
            State::Audio => Self::Audio,
            State::Pdf => Self::Pdf,
            State::Archive => Self::Archive,
            State::Text if kinds::is_markdown(&name) => Self::Markdown,
            State::Text if matches!(ext, "csv" | "tsv") => Self::Table,
            State::Text => Self::Text,
            State::Code if kinds::is_html(&name) => Self::Html,
            State::Code => Self::Code,
            _ => Self::Facts,
        }
    }
}

/// The listing's row as Facts.js reads it.
pub(crate) fn fact_row(row: &RowFacts) -> facts::Row {
    facts::Row {
        name: row.name.clone(),
        is_dir: row.dir,
        size: row.size,
        mtime: row.mtime,
        mode: row.mode,
        icon: row.icon.clone(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Subject {
    pub directory: String,
    pub listing: usize,
    pub index: usize,
    pub row: RowFacts,
    pub selected: Vec<RowFacts>,
    pub selected_count: usize,
}
impl Subject {
    pub(crate) fn for_pane(host: &super::pane::PaneHost) -> Option<Self> {
        let row = host.pane.held.row(host.pane.cursor)?;
        if row.name.is_empty() || (row.dir && host.selected.len() <= 1) {
            return None;
        }
        Some(Self {
            directory: host.pane.path().into(),
            listing: host.pane.held.listing,
            index: host.pane.cursor,
            row: row.clone(),
            selected_count: host.selected.len(),
            selected: host
                .pane
                .held
                .rows
                .iter()
                .enumerate()
                .filter(|(offset, _)| {
                    host.selected
                        .binary_search(&(host.pane.held.start + offset))
                        .is_ok()
                })
                .map(|(_, row)| row.clone())
                .collect(),
        })
    }
    fn path(&self) -> String {
        crate::navigation::history::join(&self.directory, &self.row.name)
    }
    fn kind(&self) -> Kind {
        if self.selected_count > 1 {
            Kind::Multi
        } else {
            Kind::of(&self.row)
        }
    }
}

/// The actual mounted renderer, also the entry point for subsequent document kinds.
pub enum Mounted {
    Basic,
    Pdf(Entity<PdfView>),
    Media(Option<Entity<MediaPlayer>>),
    Model(Entity<Model3dView>),
    Doc(docs::Doc),
}

pub(crate) struct Frame {
    pub subject: Subject,
    pub generation: u64,
    pub kind: Kind,
    pub mounted: Mounted,
    pub meta: Option<Json>,
    /// The meta reply as Facts.js reads it.
    pub facts: Option<Meta>,
    pub thumb_answered: bool,
    pub thumb_observation: Option<thumbnail_observer::Observation>,
    pub source: Option<gpui::Resource>,
    pub image: Option<Arc<RenderImage>>,
    /// An animated GIF's own frames, loaded from the original over its still frame.
    pub animation: Option<Arc<RenderImage>>,
    pub animation_source: Option<gpui::Resource>,
    pub quality: crate::preview_quality::Refinement,
    pub trace: Option<crate::preview_trace::Trace>,
    pub failed: bool,
    pub manual: bool,
    pub remote: bool,
    pub _watch: Option<Subscription>,
}

impl Frame {
    /// An image frame that animates: a GIF with more than one frame, its still already drawn.
    pub(crate) fn animated_gif(&self) -> bool {
        self.kind == Kind::Image
            && self.subject.row.name.to_ascii_lowercase().ends_with(".gif")
            && self.facts.as_ref().is_some_and(|meta| {
                // Every frame decodes whole up front, so the moving layer keeps to the raster budget.
                let pixels = u64::from(meta.w) * u64::from(meta.h) * u64::from(meta.frames);
                meta.frames > 1
                    && pixels > 0
                    && pixels <= crate::g2::doc_preview::poppler::MAX_PIXELS
            })
            && self.image.is_some()
    }
}

#[derive(Default)]
pub(crate) struct SwapState {
    pub observation: observation::Swap,
    pub clock: Option<observation::Clock>,
    pub now: u64,
    pub holding: bool,
    pub cap: Option<u64>,
    pub ready: bool,
    pub holds: u64,
    pub fallbacks: u64,
    pub loading_at: Option<u64>,
    pub folder: bool,
    pub wanted: bool,
    pub readers: std::collections::HashMap<String, String>,
}

impl SwapState {
    fn time(&self) -> u64 {
        self.clock.as_ref().map_or(self.now, |clock| {
            clock
                .executor
                .now()
                .saturating_duration_since(clock.origin)
                .as_millis() as u64
        })
    }
}

/// Both Columns ports borrow this same retained state, so a hold survives every Area call.
#[derive(Clone, Default)]
pub(crate) struct Port(pub Rc<RefCell<SwapState>>);
impl ThirdSwap for Port {
    fn idle(&self) -> bool {
        !self.0.borrow().holding
    }
    fn hold(&mut self, key: &str) {
        let mut state = self.0.borrow_mut();
        let now = state.time();
        let holding = state.holding;
        state.observation.hold(now, holding, key);
        if !state.holding {
            state.holds += 1;
        }
        state.holding = true;
    }
    fn start(&mut self, _: bool) {
        let mut state = self.0.borrow_mut();
        state.cap = Some(state.time() + crate::pane::swap::CAP_MS);
    }
    fn stop_cap(&mut self) {
        let mut state = self.0.borrow_mut();
        state.cap = None;
        state.folder = true;
    }
    fn cancel(&mut self) {
        let mut state = self.0.borrow_mut();
        let now = state.time();
        observation::release(&mut state, now, "cancelled", false);
        state.holding = false;
        state.cap = None;
        state.folder = false;
    }
}
impl PreviewProducer for Port {
    fn ready(&self) -> bool {
        self.0.borrow().ready
    }
    fn follow_selection(&mut self) {
        self.0.borrow_mut().wanted = true;
    }
    fn arm_settle(&mut self) {
        self.start(false);
    }
    fn ask_cursor_thumb(&mut self) {
        self.0.borrow_mut().wanted = true;
    }
    fn ipc(&self, name: &str, _: &[String]) -> Option<String> {
        self.0.borrow().readers.get(name).cloned()
    }
}

pub(crate) type Outbox = Rc<RefCell<Vec<String>>>;
pub struct SelectionPreview {
    pub(crate) port: Port,
    pub(crate) outbox: Outbox,
    pub(crate) pending: Option<Frame>,
    pub(crate) manual_subject: Option<Subject>,
    pub(crate) drawn: Option<Frame>,
    pub(crate) visible: bool,
    pub(crate) mode: super::startup::View,
    pub(crate) class: Option<(String, String)>,
    pub(crate) rect: Rc<RefCell<Option<gpui::Bounds<gpui::Pixels>>>>,
    pub(crate) frame_rect: Rc<RefCell<Option<gpui::Bounds<gpui::Pixels>>>>,
    pub(crate) play_rect: Rc<RefCell<Option<gpui::Bounds<gpui::Pixels>>>>,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) catalog: Arc<flea::i18n::I18n>,
    pub(crate) face: crate::list::Typeface,
    pub(crate) corner: f32,
    pub(crate) full_facts: bool,
    pub(crate) automatic: bool,
    pub(crate) media_backend: Option<Rc<dyn crate::g2::media_preview::engine::Backend>>,
    pub(crate) caches: crate::g2::media_preview::surface::Caches,
    pub(crate) loading_mark: Rc<crate::feedback::spinner::Crawl>,
    pub(crate) epoch: Instant,
    pub(crate) timer: Option<(u64, Task<()>)>,
    pub(crate) gif: Option<picture::GifClock>,
    pub(crate) retired: Vec<Frame>,
    pub(crate) focus: gpui::FocusHandle,
    pub(crate) frames: Rc<RefCell<frame_feedback::Frames>>,
    pub(crate) frame_feedback: Option<Subscription>,
    // The plugin host watch and the last frame generation a previewer was asked about.
    pub(crate) previewer: (Option<Subscription>, u64),
}
