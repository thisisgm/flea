//! Quick Look (pv1): the overlay's state, its ground policy and the card it draws over the listing.
mod act;
pub mod bar;
pub mod body;
pub mod button;
pub mod card;
pub mod chrome;
pub mod column;
pub mod decode;
pub mod doc;
mod doc_card;
mod doc_keys;
pub mod glyph;
pub mod look;
mod photo;
mod photo_frames;
mod photo_png;
#[cfg(test)]
mod photo_tests;
mod picture;
mod raster;
pub mod scroller;
pub mod shield;
mod svg;
mod viewport;

pub use raster::{RasterRefusal, RasterState, Rasterizer};

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
    pub(crate) languages: bool,
    pub(crate) i18n: Arc<I18n>,
    pub(crate) look: Option<Look>,
    pub(crate) subject: Option<Subject>,
    svg: Option<SvgPicture>,
    zoom: f64,
    raster: Option<Rasterizer>,
    raster_latest: Arc<AtomicU64>,
    raster_in_flight: bool,
    /// The window the card last drew in; a retired tile is freed there even after the host unmounts.
    window: Option<AnyWindowHandle>,
    retired: Vec<Arc<RenderImage>>,
    retire_scheduled: bool,
    ground: Ground,
    pub(crate) revealed: bool,
    pub(crate) expanded: bool,
    pub(crate) strip_shown: bool,
    pub(crate) focused: Option<Hit>,
    pub(crate) hovered: Option<Hit>,
    pub(crate) pressed: Option<Hit>,
    /// Survives the shield's release so a press taken while hidden cannot activate newly revealed chrome.
    pub(crate) chrome_armed: bool,
    pub(crate) scroll: Scroll,
    /// The mounted view's notifications: each re-reads its readiness, its page shape and the PDF's opening page.
    doc_watch: Option<gpui::Subscription>,
}

impl EventEmitter<Event> for QuickLook {}

impl QuickLook {
    pub fn new(i18n: Arc<I18n>) -> Self {
        Self {
            languages: true,
            i18n,
            look: None,
            subject: None,
            svg: None,
            zoom: 1.,
            raster: None,
            raster_latest: Arc::new(AtomicU64::new(0)),
            raster_in_flight: false,
            window: None,
            retired: Vec::new(),
            retire_scheduled: false,
            ground: Ground::default(),
            revealed: false,
            expanded: false,
            strip_shown: false,
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
        let moved = self
            .subject
            .as_ref()
            .and_then(|s| s.body.doc())
            .filter(|old| subject.body.doc().is_none_or(|new| new.id() != old.id()));
        if let Some(old) = moved.cloned() {
            old.close(cx);
        }
        self.doc_watch = subject.body.doc().map(|doc| doc.observe(cx));
        self.subject = Some(subject);
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
        self.revealed = true;
        cx.notify();
    }

    /// close() in Preview.qml: every per-subject flag goes, so the next open starts clean.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        let Some(subject) = self.subject.take() else {
            return;
        };
        self.doc_watch = None;
        if let Some(doc) = subject.body.doc() {
            doc.close(cx);
        }
        self.ground.reset();
        self.retire_svg(cx);
        self.zoom = 1.;
        self.revealed = false;
        self.expanded = false;
        self.strip_shown = false;
        self.focused = None;
        self.hovered = None;
        self.pressed = None;
        self.chrome_armed = false;
        self.scroll = Scroll::default();
        cx.emit(Event::Closed);
        cx.notify();
    }

    /// One pointer event the shield already consumed: the viewport's wheel, lane and knob first, then the ground's policy.
    pub fn pointer(&mut self, hand: Hand, cx: &mut Context<Self>) {
        let Hand {
            pointer,
            at,
            inside,
            wheel,
            held,
        } = hand;
        match pointer {
            Pointer::Release { .. } => {
                let held = self.pressed.take().is_some();
                if self.scroll.release() || held {
                    cx.notify();
                }
            }
            Pointer::Move => {
                let (changed, dragged) = self.scroll.hover(at, held);
                if let Some(axis) = dragged {
                    self.hold(axis, cx);
                }
                if changed {
                    cx.notify();
                }
            }
            Pointer::Press {
                button: Button::Left,
                ..
            } if self.revealed && inside => {
                if let Some(axis) = self.scroll.press(at) {
                    self.hold(axis, cx);
                }
            }
            Pointer::Wheel if self.revealed => {
                for axis in wheel.map(|d| self.scroll.wheel(at, d)).unwrap_or_default() {
                    self.hold(axis, cx);
                }
            }
            _ => {}
        }
        match self.ground.handle(pointer, inside, self.revealed) {
            Effect::Swallow => {}
            Effect::Reveal => self.reveal(cx),
            Effect::RevealStrip => {
                if !self.strip_shown {
                    self.strip_shown = true;
                    cx.notify();
                }
            }
            Effect::Close => self.close(cx),
        }
    }

    /// The view moved on an axis: its bar shows now and leaves HOLD_MS after the last move.
    fn hold(&mut self, axis: Axis, cx: &mut Context<Self>) {
        let generation = self.scroll.moved(axis);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(flea::ui_logic_pv1::scroll::HOLD_MS))
                .await;
            let _ = this.update(cx, |ql, cx| {
                if ql.scroll.settle(axis, generation) {
                    cx.notify();
                }
            });
        })
        .detach();
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
