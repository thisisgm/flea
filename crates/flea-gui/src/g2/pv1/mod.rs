//! Quick Look (pv1): the overlay's state, its ground policy and the card it draws over the listing.
pub mod bar;
pub mod body;
pub mod button;
pub mod card;
pub mod column;
pub mod glyph;
pub mod look;
mod raster;
pub mod scroller;
pub mod shield;
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
    pub(crate) scroll: Scroll,
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
            scroll: Scroll::default(),
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
        self.subject = Some(subject);
        self.retire_svg(cx);
        self.revealed = revealed;
        self.focused = None;
        self.pressed = None;
        self.scroll = Scroll::default();
        cx.notify();
    }

    /// The swap's first whole frame: the card draws.
    pub fn reveal(&mut self, cx: &mut Context<Self>) {
        self.revealed = true;
        cx.notify();
    }

    /// The open vector's raster at the drawn fit times the window scale (#336); a raster file draws itself.
    fn svg_image(
        &mut self,
        path: &Path,
        fit_w: f64,
        fit_h: f64,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> RasterState {
        use flea::ui_logic_pv1::pixels::{decode_size, is_vector_name, needs_redecode};
        let vector = path.file_name().and_then(|n| n.to_str()).is_some_and(is_vector_name);
        if !vector {
            return RasterState::default();
        }
        let scale = f64::from(window.scale_factor());
        let (dw, dh) = decode_size(fit_w, fit_h, scale, 0.0, 0.0, true);
        if dw == 0 || dh == 0 {
            return RasterState::default();
        }
        let fresh = self.svg.as_ref().is_some_and(|s| {
            s.path == path && s.size == (dw, dh) && !needs_redecode(s.scale, scale)
        });
        if fresh {
            return self.svg.as_ref().map(SvgPicture::state).unwrap_or_default();
        }
        // The same path keeps drawing its last raster until the new one lands; a new path retires it.
        let prior = self.svg.take();
        let keep = prior.as_ref().filter(|s| s.path == path).and_then(|s| s.image.clone());
        if let Some(prior) = prior {
            if prior.path != path {
                self.retire(prior.image, cx);
            }
        }
        self.svg = Some(SvgPicture {
            path: path.to_path_buf(),
            scale,
            size: (dw, dh),
            image: keep,
            refusal: None,
        });
        // A new request invalidates the job in flight; the next one starts when that job ends.
        self.raster_latest.fetch_add(1, Ordering::SeqCst);
        self.start_raster(cx);
        self.svg.as_ref().map(SvgPicture::state).unwrap_or_default()
    }

    /// Starts one raster job for the current request; a job already in flight finishes first.
    fn start_raster(&mut self, cx: &mut Context<Self>) {
        if self.raster_in_flight {
            return;
        }
        let Some(svg) = self.svg.as_ref() else {
            return;
        };
        let (path, size) = (svg.path.clone(), svg.size);
        let epoch = self.raster_latest.load(Ordering::SeqCst);
        let raster = self
            .raster
            .clone()
            .unwrap_or_else(|| raster::default_raster(cx.svg_renderer()));
        self.raster_in_flight = true;
        // The read, parse and raster run on the background executor, never in the frame that asked.
        let job = raster::Job {
            path: path.clone(),
            size,
            epoch,
            latest: self.raster_latest.clone(),
            raster,
        };
        let work = cx.background_executor().spawn(async move { raster::run(&job) });
        cx.spawn(async move |this, cx| {
            let landed = work.await;
            let _ = this.update(cx, |ql, cx| {
                ql.raster_in_flight = false;
                let current = ql.raster_latest.load(Ordering::SeqCst) == epoch
                    && ql.svg.as_ref().is_some_and(|s| s.path == path && s.size == size);
                if !current {
                    // This request moved on; the latest wanted size starts now, still one job at a time.
                    ql.start_raster(cx);
                    return;
                }
                match landed {
                    raster::RasterOutcome::Raster(image) => {
                        if let Some(svg) = ql.svg.as_mut() {
                            svg.refusal = None;
                            if let Some(old) = svg.image.replace(image) {
                                ql.retire(Some(old), cx);
                            }
                        }
                    }
                    raster::RasterOutcome::Refused(refusal) => {
                        if let Some(svg) = ql.svg.as_mut() {
                            svg.refusal = Some(refusal);
                        }
                    }
                    raster::RasterOutcome::Superseded => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Puts a raster the card no longer draws on the retire list for the window that holds its tile.
    fn retire(&mut self, image: Option<Arc<RenderImage>>, cx: &mut Context<Self>) {
        if let Some(image) = image {
            self.retired.push(image);
        }
        self.schedule_retire(cx);
    }

    /// Frees retired tiles at the end of this effect cycle, so a close frees its tile before the next frame.
    fn schedule_retire(&mut self, cx: &mut Context<Self>) {
        if self.retired.is_empty() || self.retire_scheduled {
            return;
        }
        let Some(window) = self.window else {
            return;
        };
        self.retire_scheduled = true;
        let entity = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, app| {
                let _ = entity.update(app, |ql, _| {
                    ql.retire_scheduled = false;
                    for image in std::mem::take(&mut ql.retired) {
                        let _ = window.drop_image(image);
                    }
                });
            });
        });
    }

    /// Retires the open vector's raster and invalidates every in-flight request for it.
    fn retire_svg(&mut self, cx: &mut Context<Self>) {
        let image = self.svg.take().and_then(|s| s.image);
        self.retire(image, cx);
        let _ = self.raster_latest.fetch_add(1, Ordering::SeqCst);
    }

    /// close() in Preview.qml: every per-subject flag goes, so the next open starts clean.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.subject.take().is_none() {
            return;
        }
        self.ground.reset();
        self.retire_svg(cx);
        self.revealed = false;
        self.expanded = false;
        self.strip_shown = false;
        self.focused = None;
        self.hovered = None;
        self.pressed = None;
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

    /// A card control answered; a disabled one never reaches here.
    pub fn activate(&mut self, hit: Hit, cx: &mut Context<Self>) {
        match hit {
            Hit::Close => self.close(cx),
            Hit::OpenWith => cx.emit(Event::OpenWith),
            Hit::CopyPatch => cx.emit(Event::CopyPatch),
            Hit::Control(index) => {
                let glyph = self
                    .subject
                    .as_ref()
                    .and_then(|s| s.body.controls().get(index).copied());
                if glyph.is_some_and(|c| c.enabled && c.glyph == glyph::Glyph::Maximize) {
                    self.expanded = !self.expanded;
                    cx.notify();
                }
            }
        }
    }

    /// The PDF strip's walk when PdfViewer holds the keyboard: its controls, then Open With, the pane's one action, then Close.
    fn strip(&self) -> Option<Vec<(Hit, bool)>> {
        let controls = self.subject.as_ref()?.body.controls();
        if controls.is_empty() {
            return None;
        }
        let mut walk: Vec<(Hit, bool)> = controls
            .iter()
            .enumerate()
            .map(|(i, c)| (Hit::Control(i), c.enabled))
            .collect();
        walk.extend([(Hit::OpenWith, true), (Hit::Close, true)]);
        Some(walk)
    }

    /// PdfViewer's keys (PreviewKeys.pdfAction); Space and Escape still close every kind.
    fn strip_action(&mut self, walk: &[(Hit, bool)], action: &str, cx: &mut Context<Self>) -> Command {
        if matches!(action, "preview" | "escape" | "focusPreview") {
            self.close(cx);
            return Command::Close;
        }
        let controls: Vec<keys::Control> = walk
            .iter()
            .map(|&(_, enabled)| keys::Control {
                enabled,
                visible: true,
            })
            .collect();
        let index = self
            .focused
            .and_then(|f| walk.iter().position(|(h, _)| *h == f))
            .map_or(-1, |i| i as i32);
        match keys::pdf_action(action, Some(&controls), index) {
            PdfCommand::Focus(i) => {
                self.focused = walk.get(i as usize).map(|(h, _)| *h);
                cx.notify();
                Command::Nothing
            }
            PdfCommand::Activate(i) => {
                self.activate(walk[i].0, cx);
                Command::Nothing
            }
            PdfCommand::ToggleExpand => {
                self.expanded = !self.expanded;
                cx.notify();
                Command::ToggleExpand
            }
            // A PDF that could not be read has no page to turn, zoom or scroll.
            _ => Command::Nothing,
        }
    }

    /// One named action while Quick Look holds the keyboard.
    pub fn action(&mut self, action: &str, cx: &mut Context<Self>) -> Command {
        if let Some(walk) = self.strip() {
            return self.strip_action(&walk, action, cx);
        }
        let subject = self.subject.as_ref();
        let has_bar = subject.is_some_and(|s| s.body.has_bar());
        let surface = Surface {
            is_media: subject.is_some_and(|s| matches!(s.state, State::Video | State::Audio)),
            is_pdf: subject.is_some_and(|s| s.state == State::Pdf),
            is_compare: subject.is_some_and(|s| s.body.is_compare()),
            has_bar,
            close_focused: self.focused == Some(Hit::Close),
            ..Surface::default()
        };
        let open_with = subject.is_some_and(|s| s.body.offers_open_with());
        let command = keys::act(action, surface);
        match command {
            Command::Close => self.close(cx),
            Command::Follow(step) => cx.emit(Event::Follow(step)),
            Command::ToggleExpand => {
                self.expanded = !self.expanded;
                cx.notify();
            }
            Command::CopyPatch => cx.emit(Event::CopyPatch),
            // Compare's Tab walk: Copy Patch, then Close, round again; a refusal has Close alone.
            Command::CompareFocus(back) => {
                let walk: &[Hit] = if subject.is_some_and(|s| matches!(s.body, Body::Diff { .. })) {
                    &[Hit::CopyPatch, Hit::Close]
                } else {
                    &[Hit::Close]
                };
                let at = self.focused.and_then(|f| walk.iter().position(|w| *w == f));
                let next = match (at, back) {
                    (None, false) => 0,
                    (None, true) => walk.len() - 1,
                    (Some(i), false) => (i + 1) % walk.len(),
                    (Some(i), true) => (i + walk.len() - 1) % walk.len(),
                };
                self.focused = Some(walk[next]);
                cx.notify();
            }
            Command::Nothing if action == "open" && self.focused.is_some() => {
                if let Some(hit) = self.focused {
                    self.activate(hit, cx);
                }
            }
            Command::ToggleCloseFocus => {
                self.focused = (self.focused != Some(Hit::Close)).then_some(Hit::Close);
                cx.notify();
            }
            // With no bar, Tab walks to the card's one action, the DialogButton focus a framed button takes.
            Command::Nothing if open_with && !has_bar && action.starts_with("focus") => {
                self.focused = (self.focused != Some(Hit::OpenWith)).then_some(Hit::OpenWith);
                cx.notify();
            }
            _ => {}
        }
        command
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
