//! One retained selection preview per pane; completed frames survive replacement work.
use crate::{
    columns::preview_host::{PreviewProducer, ThirdSwap},
    g2::{pv2::PdfView, pv3::player::MediaPlayer, pv4::view::Model3dView},
};
use flea::{jsondoc::Json, protocol::RowFacts};
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

#[path = "selection_preview_frames.rs"]
mod frame_feedback;
#[path = "selection_preview_host.rs"]
pub(crate) mod host;
#[path = "selection_preview_lifetime.rs"]
mod lifetime;
#[path = "selection_preview_load.rs"]
mod load;
#[path = "selection_preview_readers.rs"]
pub(crate) mod readers;
#[path = "selection_preview_render.rs"]
mod render;
pub(crate) use render::number;

static NEXT: AtomicU64 = AtomicU64::new(1);
fn token() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Later document hosts extend this enum and Mounted, sharing this owner's lifecycle and backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Image,
    Text,
    Pdf,
    Video,
    Audio,
    Model,
    Facts,
    Multi,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Text => "text",
            Self::Pdf => "pdf",
            Self::Video => "video",
            Self::Audio => "audio",
            Self::Model => "model3d",
            Self::Facts => "unsupported",
            Self::Multi => "multi",
        }
    }
    pub(crate) fn tall(self) -> bool {
        matches!(self, Self::Text | Self::Pdf)
    }
    fn of(row: &RowFacts) -> Self {
        let ext = row
            .name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "pdf" => Self::Pdf,
            "stl" | "3mf" | "obj" | "ply" | "amf" | "gcode" | "gco" | "bgcode" => Self::Model,
            "mp4" | "mkv" | "webm" | "mov" | "avi" | "m4v" => Self::Video,
            "mp3" | "ogg" | "oga" | "opus" | "wav" | "flac" | "m4a" | "aac" => Self::Audio,
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "avif" | "bmp" | "svg" | "svgz" | "tiff"
            | "tif" => Self::Image,
            "txt" | "rs" | "py" | "c" | "h" | "cpp" | "js" | "ts" | "json" | "toml" | "yaml"
            | "yml" | "sh" | "css" | "log" => Self::Text,
            _ if row.icon.starts_with("image-") => Self::Image,
            _ if row.icon.starts_with("audio-") => Self::Audio,
            _ if row.icon.starts_with("video-") => Self::Video,
            _ if row.icon.starts_with("text-") => Self::Text,
            _ => Self::Facts,
        }
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
}

pub(crate) struct Frame {
    pub subject: Subject,
    pub generation: u64,
    pub kind: Kind,
    pub mounted: Mounted,
    pub meta: Option<Json>,
    pub lines: Rc<[String]>,
    pub text_cache: Rc<RefCell<super::preview_text::Cache>>,
    pub text_answered: bool,
    pub thumb_answered: bool,
    pub source: Option<gpui::Resource>,
    pub image: Option<Arc<RenderImage>>,
    pub failed: bool,
    pub manual: bool,
    pub _watch: Option<Subscription>,
}

#[derive(Default)]
pub(crate) struct SwapState {
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

/// Both Columns ports borrow this same retained state, so a hold survives every Area call.
#[derive(Clone, Default)]
pub(crate) struct Port(pub Rc<RefCell<SwapState>>);
impl ThirdSwap for Port {
    fn idle(&self) -> bool {
        !self.0.borrow().holding
    }
    fn hold(&mut self, _: &str) {
        let mut state = self.0.borrow_mut();
        if !state.holding {
            state.holds += 1;
        }
        state.holding = true;
    }
    fn start(&mut self, _: bool) {
        let mut state = self.0.borrow_mut();
        state.cap = Some(state.now + crate::pane::swap::CAP_MS);
    }
    fn stop_cap(&mut self) {
        let mut state = self.0.borrow_mut();
        state.cap = None;
        state.folder = true;
    }
    fn cancel(&mut self) {
        let mut state = self.0.borrow_mut();
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
    pub(crate) media_backend: Option<Rc<dyn crate::g2::pv3::engine::Backend>>,
    pub(crate) caches: crate::g2::pv3::surface::Caches,
    pub(crate) epoch: Instant,
    pub(crate) timer: Option<(u64, Task<()>)>,
    pub(crate) retired: Vec<Frame>,
    pub(crate) focus: gpui::FocusHandle,
    pub(crate) frames: Rc<RefCell<frame_feedback::Frames>>,
    pub(crate) frame_feedback: Option<Subscription>,
}
impl SelectionPreview {
    pub(crate) fn new(
        port: Port,
        catalog: Arc<flea::i18n::I18n>,
        face: crate::list::Typeface,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            port,
            outbox: Rc::default(),
            pending: None,
            manual_subject: None,
            drawn: None,
            visible: false,
            mode: super::startup::View::List,
            class: None,
            rect: Rc::default(),
            frame_rect: Rc::default(),
            play_rect: Rc::default(),
            width: 0.,
            height: 0.,
            catalog,
            face,
            corner: 0.,
            full_facts: false,
            media_backend: None,
            caches: Default::default(),
            epoch: cx.background_executor().now(),
            timer: None,
            retired: Vec::new(),
            focus: cx.focus_handle(),
            frames: Rc::default(),
            frame_feedback: None,
        }
    }
    pub(crate) fn now(&self, cx: &gpui::App) -> u64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.epoch)
            .as_millis() as u64
    }
    pub fn mounted_kind(&self) -> Option<(&Kind, &Mounted)> {
        self.drawn.as_ref().map(|f| (&f.kind, &f.mounted))
    }
}
