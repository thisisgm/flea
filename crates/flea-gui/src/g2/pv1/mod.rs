//! Quick Look (pv1): the overlay's state, its ground policy and the card it draws over the listing.
mod act;
pub mod bar;
pub mod body;
pub mod button;
pub mod card;
mod catalog;
pub mod chrome;
pub mod column;
pub mod decode;
pub mod doc;
mod doc_card;
mod doc_keys;
pub mod glyph;
mod hold;
mod lifetime;
pub mod look;
mod media_strip;
pub(crate) mod observations;
pub(crate) mod observed_frames;
pub(crate) mod photo;
pub(crate) mod photo_frames;
mod photo_png;
mod photo_png_first;
pub(crate) mod photo_source;
#[cfg(test)]
mod photo_source_tests;
#[cfg(test)]
mod photo_tests;
mod picture;
mod pointer;
#[cfg(test)]
mod quality_tests;
pub(crate) mod raster;
mod resize;
#[cfg(test)]
mod reuse_tests;
mod sample;
pub mod scroller;
pub mod shield;
mod svg;
mod viewport;
pub(crate) mod waiting;

pub use raster::{
    for_path as raster_for_path, run as raster_run, Job as RasterJob, RasterOutcome, RasterRefusal,
    RasterState, Rasterizer,
};

use flea::i18n::I18n;
use flea::ui_logic_pv1::{
    ground::{Button, Effect, Ground, Pointer},
    keys::{self, Command, PdfCommand, Surface},
    kinds::State,
    rect::{self, Shape, TOKENS},
};
use gpui::{point, px, size, AnyWindowHandle, Bounds, Context, EventEmitter, Pixels, RenderImage};
use scroller::{Axis, Scroll};
use shield::Hand;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

pub use body::{Body, Control, Subject};
pub use doc_keys::Outcome;
pub use look::Look;

/// What the overlay asks of its host; the listing never sees the pointer event that caused one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Closed,
    OpenWith,
    CopyPatch,
    Follow(i32),
}

/// A control on the card that takes the pointer and the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Control(usize),
    /// A mounted view's bar word: Rendered or Source, a sheet, a plate.
    Word(usize),
    /// A mounted view's strip control, by its index in the view's own strip.
    Strip(usize),
    CopyPatch,
    Close,
    OpenWith,
}

/// An open vector picture rasterized for one request size: its path, scale and pixels (#336).
struct SvgPicture {
    path: PathBuf,
    scale: f64,
    size: (u32, u32),
    image: Option<Arc<RenderImage>>,
    refusal: Option<RasterRefusal>,
    quality: crate::preview_quality::Refinement,
    interim: bool,
}

impl SvgPicture {
    /// The raster as a host holds it, keyed by the device size this request asked for.
    fn state(&self) -> RasterState {
        RasterState {
            size: Some(self.size),
            image: self.image.clone(),
            refusal: self.refusal,
        }
    }
}

pub struct QuickLook {
    pub(crate) motion: crate::reveal::Reveal,
    pub(crate) closing: Option<Subject>,
    held: Option<hold::Held>,
    /// A move from a shown card keeps the reveal clock at shown, so only open and close fade.
    pub(crate) kept_shown: bool,
    close_task: Option<gpui::Task<()>>,
    pub(crate) observations: observations::Shared,
    observation_origin: Option<std::time::Instant>,
    observation_frames: std::rc::Rc<std::cell::RefCell<observed_frames::Frames>>,
    observation_feedback: Option<gpui::Subscription>,
    pub(crate) languages: bool,
    pub(crate) i18n: Arc<I18n>,
    pub(crate) look: Option<Look>,
    pub(crate) subject: Option<Subject>,
    svg: Option<SvgPicture>,
    trace: Option<crate::preview_trace::Trace>,
    zoom: f64,
    raster: Option<Rasterizer>,
    raster_latest: Arc<AtomicU64>,
    raster_in_flight: bool,
    /// The window the card last drew in; a retired tile is freed there even after the host unmounts.
    window: Option<AnyWindowHandle>,
    /// LATENCY-ATOMIC's hold ran out with the subject's answer still out, so the waiting mark draws.
    waiting_due: bool,
    waiting_hold: Option<gpui::Task<()>>,
    retired: Vec<Arc<RenderImage>>,
    /// Every tile freed from the atlas, so a test can tell a freed interim from a kept one.
    #[cfg(test)]
    pub(crate) retired_ids: Vec<gpui::ImageId>,
    retire_scheduled: bool,
    ground: Ground,
    pub(crate) revealed: bool,
    pub(crate) expanded: bool,
    pub(crate) strip_shown: bool,
    strip_hide_task: Option<gpui::Task<()>>,
    pub(crate) focused: Option<Hit>,
    pub(crate) hovered: Option<Hit>,
    pub(crate) pressed: Option<Hit>,
    /// Survives the shield's release so a press taken while hidden cannot activate newly revealed chrome.
    pub(crate) chrome_armed: bool,
    pub(crate) scroll: Scroll,
    /// The mounted view's notifications: each re-reads its readiness, its page shape and the PDF's opening page.
    doc_watch: Option<gpui::Subscription>,
    /// The view the host takes back as its warm entry, so the close leaves it built.
    pub(crate) kept: Option<gpui::EntityId>,
}

impl EventEmitter<Event> for QuickLook {}

impl QuickLook {
    pub fn new(i18n: Arc<I18n>) -> Self {
        Self {
            motion: Default::default(),
            closing: None,
            held: None,
            kept_shown: false,
            kept: None,
            close_task: None,
            observations: Default::default(),
            observation_origin: None,
            observation_frames: Default::default(),
            observation_feedback: None,
            languages: true,
            i18n,
            look: None,
            subject: None,
            svg: None,
            trace: None,
            zoom: 1.,
            raster: None,
            raster_latest: Arc::new(AtomicU64::new(0)),
            raster_in_flight: false,
            window: None,
            waiting_due: false,
            waiting_hold: None,
            retired: Vec::new(),
            #[cfg(test)]
            retired_ids: Vec::new(),
            retire_scheduled: false,
            ground: Ground::default(),
            revealed: false,
            expanded: false,
            strip_shown: false,
            strip_hide_task: None,
            focused: None,
            hovered: None,
            pressed: None,
            chrome_armed: false,
            scroll: Scroll::default(),
            doc_watch: None,
        }
    }

    /// The host's theme roles; until it sets them the card draws from the inherited palette.
    pub fn set_look(&mut self, look: Look, cx: &mut Context<Self>) {
        self.look = Some(look);
        cx.notify();
    }

    /// The roles the card draws with: the host's when it set them, else the theme's inherited roles.
    pub(crate) fn look(&self, style: &gpui::TextStyle, theme: crate::palette::Theme) -> Look {
        self.look.unwrap_or_else(|| Look::of_text(style, theme))
    }

    pub(crate) fn visible(&self) -> bool {
        self.subject.is_some() || self.closing.is_some()
    }

    pub fn is_open(&self) -> bool {
        self.subject.is_some()
    }

    pub fn subject(&self) -> Option<&Subject> {
        self.subject.as_ref()
    }

    pub fn is_revealed(&self) -> bool {
        self.revealed
    }

    pub fn focused(&self) -> Option<Hit> {
        self.focused
    }

    pub fn pressed(&self) -> Option<Hit> {
        self.pressed
    }

    pub fn hovered(&self) -> Option<Hit> {
        self.hovered
    }

    /// The raster the card currently holds for its open vector, test support for the #336 retention check.
    #[doc(hidden)]
    pub fn svg_raster(&self) -> Option<Arc<RenderImage>> {
        self.svg.as_ref().and_then(|s| s.image.clone())
    }

    pub(crate) fn quality_pixels(&self) -> Option<((u32, u32), bool)> {
        let picture = self.svg.as_ref()?;
        match &picture.quality.requested {
            Some(request) => Some((request.size, picture.quality.ready)),
            None => picture.quality.ready.then_some((picture.size, true)),
        }
    }

    /// Source decodes of the open picture's file, for the decodes-per-open reader.
    pub(crate) fn quality_decodes(&self) -> usize {
        self.svg.as_ref().map_or(0, |picture| {
            crate::preview_quality::reuse::decodes(&picture.path)
        })
    }

    #[doc(hidden)]
    pub fn svg_refused(&self) -> bool {
        self.svg.as_ref().is_some_and(|s| s.refusal.is_some())
    }

    /// An injected rasterizer for #336's tests; unset, the app's SVG renderer runs.
    #[doc(hidden)]
    pub fn set_svg_raster(&mut self, raster: Rasterizer) {
        self.raster = Some(raster);
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Opens on a subject; revealed is false while the swap still holds the card back.
    pub fn open(&mut self, subject: Subject, revealed: bool, cx: &mut Context<Self>) {
        self.close_task = None;
        if let Some(closing) = self.closing.take() {
            self.expanded = false;
            self.strip_shown = false;
            if let Some(old) = closing
                .body
                .doc()
                .filter(|old| subject.body.doc().is_none_or(|new| new.id() != old.id()))
            {
                old.close(cx);
            }
        }
        // A move from a shown card stays shown; the old card draws until the new subject reveals.
        let moving = self.subject.is_some() && (self.revealed || self.held.is_some());
        self.kept_shown = moving;
        // A view drawn twice in one frame is not held: the same mounted view simply redraws.
        let same_view = self
            .subject
            .as_ref()
            .and_then(|old| old.body.doc())
            .zip(subject.body.doc())
            .is_some_and(|(old, new)| old.id() == new.id());
        if moving && !revealed && self.held.is_none() && !same_view {
            self.hold_card(cx);
        }
        let now = self.observation_time(cx);
        self.observations.borrow_mut().opened(now, revealed);
        // A held subject is gone from here; its view closes when the hold lets go.
        let moved = self
            .subject
            .as_ref()
            .and_then(|s| s.body.doc())
            .filter(|old| subject.body.doc().is_none_or(|new| new.id() != old.id()));
        if let Some(old) = moved.cloned() {
            old.close(cx);
        }
        self.doc_watch = subject.body.doc().map(|doc| doc.observe(cx));
        self.trace = crate::preview_trace::Trace::begin("look", &subject.name);
        self.subject = Some(subject);
        // Let go only after the new subject is in place, so a held view it reuses stays open.
        if !moving || revealed {
            self.release_held(cx);
        }
        self.strip_hide_task = None;
        self.strip_shown = false;
        self.reveal_strip(cx);
        self.arm_waiting(cx);
        self.retire_svg(cx);
        self.zoom = 1.;
        self.revealed = revealed;
        self.focused = None;
        self.pressed = None;
        self.chrome_armed = false;
        self.scroll = Scroll::default();
        cx.notify();
    }

    /// The swap's first whole frame: the card draws.
    pub fn reveal(&mut self, cx: &mut Context<Self>) {
        self.observe_reveal(false, cx);
        self.mark_revealed(cx);
    }

    /// The new subject draws from this frame on, and the card a move held is let go in the same frame.
    pub(crate) fn mark_revealed(&mut self, cx: &mut Context<Self>) {
        self.revealed = true;
        self.release_held(cx);
        cx.notify();
    }

    /// The card's window rect for a window of this size, the same arithmetic as Theme.qml.
    pub fn card(&self, window: gpui::Size<Pixels>) -> Bounds<Pixels> {
        let (w, h) = (
            f64::from(f32::from(window.width)),
            f64::from(f32::from(window.height)),
        );
        let shape = self
            .subject
            .as_ref()
            .map_or(Shape::unknown(State::Unsupported), |s| s.shape);
        let want = rect::surface(&shape, w, h, self.expanded, TOKENS);
        let (span_w, span_h) = (rect::card_span(want.w, w), rect::card_span(want.h, h));
        let at = |room, span| px(rect::card_origin(room, span) as f32);
        Bounds::new(
            point(at(w, span_w), at(h, span_h)),
            size(px(span_w as f32), px(span_h as f32)),
        )
    }
}
