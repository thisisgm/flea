//! The PDF surface as a GPUI view: the preview column's continuous stack or Quick Look's single page, painted as images.
use super::render::{Renderer, Reply};
use super::surface::{Command, Surface};
use crate::pane::swap::{CAP_MS, MIN_SHOW_MS};
use flea::i18n::I18n;
use flea::ui_logic_doc_preview::viewer;
use gpui::{App, Context, Hsla, ScrollWheelEvent, Task, Window};
use std::{cell::Cell, time::Instant};

/// The theme roles the surface reads: the window ground decides whether the white page takes its veil.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
}

impl Palette {
    /// In a dark theme the white page sits under a 5% veil of the window ground so it does not glow.
    pub fn veil(&self) -> Option<Hsla> {
        (self.background.l < 0.5).then(|| self.background.opacity(0.05))
    }
}

/// The spacing tokens the surface lays out with, from the host's scaled theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// The gap between two pages of the stack.
    pub gap: f32,
    /// Quick Look's inset round the page, twice the row padding.
    pub margin: f32,
    /// One scroll step of j and k.
    pub row_height: f32,
}

/// The host's slow storage copy, `pdfcopy` on the backend; its reply comes back through `PdfView::copied`.
pub trait CopyHost {
    fn pdf_copy(&self, path: &str, slot: &str) -> u64;
}

/// Quick Look's strip as the host's chrome bar draws it, every string already through the catalog.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    /// The counter beside the pager, "1 / 3", None until the pages are known.
    pub counter: Option<String>,
    /// The controls in drawing order with their glyph names and accessible names.
    pub items: Vec<(viewer::Item, &'static str, String)>,
}

pub struct PdfView {
    pub surface: Surface,
    bars: crate::g2::quick_look_overlay::scroller::overlay::Overlay,
    renderer: Renderer,
    origin: Instant,
    pub palette: Palette,
    pub metrics: Metrics,
    /// Quick Look's controls and zoom; None for the column's stack.
    pub viewer: Option<viewer::Viewer>,
    /// Quick Look's pan inside the zoomed frame, logical pixels from the top left.
    pub pan: (f64, f64),
    /// The viewport the surface was last laid out in, logical pixels.
    pub frame: (f64, f64),
    copy: Option<(Box<dyn CopyHost>, &'static str)>,
    timer: Option<(u64, Task<()>)>,
    frame_asked: bool,
    _pump: Task<()>,
    /// Frames painted, for the native run's idle count.
    pub painted: Cell<usize>,
    /// An office preview's cut, set by the office view; the counter reads "30+" once the pages reach it.
    pub first: Option<usize>,
}

impl Surface {
    pub fn page_count(&self) -> usize {
        self.sizes.len()
    }

    /// The page on screen, None before the first render of this document; the host's swap waits for it.
    pub fn shown_page(&self) -> Option<usize> {
        self.cap.shown_page
    }

    pub fn drawn_page(&self) -> usize {
        self.cap.drawn_page(self.page)
    }

    pub fn failed(&self) -> bool {
        self.open_failed
            || matches!(
                self.fetch,
                flea::ui_logic_doc_preview::pdf::Fetch::Failed { .. }
            )
            || self.page_failed
    }

    /// A render in flight for what the frame shows; the single page's neighbours render ahead without holding a press.
    pub fn render_busy(&self) -> bool {
        if self.continuous {
            !self.inflight.is_empty()
        } else {
            self.inflight.iter().any(|&(page, _)| page == self.page)
        }
    }
}

impl PdfView {
    pub(crate) fn cohesion_lane(&self) -> Option<flea::jsondoc::Json> {
        self.bars
            .painted_lane()
            .map(|lane| lane.control("PreviewColumn"))
    }
    /// A surface with its own render thread; replies land on the foreground as they come.
    pub fn new(
        continuous: bool,
        palette: Palette,
        metrics: Metrics,
        cx: &mut Context<Self>,
    ) -> Self {
        let renderer = Renderer::spawn();
        let replies = renderer.replies.clone();
        let pump = cx.spawn(async move |this, cx| {
            while let Ok(reply) = replies.recv().await {
                if this.update(cx, |view, cx| view.apply(reply, cx)).is_err() {
                    break;
                }
            }
        });
        // A view dropped while its window lives takes its pages out of every window's atlas.
        cx.on_release(|view, cx| {
            for image in view.surface.cache.drain() {
                cx.drop_image(image, None);
            }
        })
        .detach();
        PdfView {
            surface: Surface::new(continuous, CAP_MS, MIN_SHOW_MS),
            bars: Default::default(),
            renderer,
            origin: cx.background_executor().now(),
            palette,
            metrics,
            viewer: (!continuous).then(viewer::Viewer::default),
            pan: (0., 0.),
            frame: (0., 0.),
            copy: None,
            timer: None,
            frame_asked: false,
            _pump: pump,
            painted: Cell::new(0),
            first: None,
        }
    }

    /// Ends pending work and releases cached pictures after existing draw references retire.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.timer = None;
        self.bars = Default::default();
        self.surface.close();
        self.flush(cx);
        for image in self.surface.cache.take_released() {
            cx.defer(move |cx| cx.drop_image(image, None));
        }
    }

    pub fn now(&self, cx: &App) -> u64 {
        cx.background_executor()
            .now()
            .saturating_duration_since(self.origin)
            .as_millis() as u64
    }

    /// The backend a slow document is fetched through, and which viewer this is so cleanup touches only its copy.
    pub fn set_copy_host(
        &mut self,
        host: Box<dyn CopyHost>,
        slot: &'static str,
        fetch_first: bool,
    ) {
        self.copy = Some((host, slot));
        self.surface.set_fetch(fetch_first, true);
    }

    pub fn set_fetch_first(&mut self, fetch_first: bool) {
        self.surface.set_fetch(fetch_first, self.copy.is_some());
    }

    /// The subject: a new path resets Quick Look's zoom; it opens on the next frame when idle, at the settle otherwise.
    pub fn set_path(&mut self, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        self.bars = Default::default();
        if let Some(viewer) = &mut self.viewer {
            viewer.subject_changed();
            self.pan = (0., 0.);
        }
        self.surface.set_path(path, now);
        self.arm(window, cx);
    }

    pub fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        let now = self.now(cx);
        self.surface.set_active(active, now);
        self.arm(window, cx);
    }

    // The fast path waits for the next presented frame; every deadline gets one scheduled wake.
    fn arm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.surface.opener.fast_armed && !self.frame_asked {
            self.frame_asked = true;
            cx.on_next_frame(window, |view, _, cx| {
                view.frame_asked = false;
                view.surface.frame();
                view.flush(cx);
                cx.notify();
            });
            window.refresh();
        }
        self.schedule(cx);
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        let Some(due) = self.surface.next_deadline() else {
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
                if view.surface.tick(now) {
                    cx.notify();
                }
                view.flush(cx);
            });
        });
        self.timer = Some((due, task));
    }

    // Sends what the surface asked for: opens, closes, renders and the host's fetch.
    fn flush(&mut self, cx: &mut Context<Self>) {
        for command in self.surface.take_commands() {
            match command {
                Command::Open(path) => {
                    let doc = self.renderer.open(path, self.surface.continuous);
                    self.surface.opening(doc);
                }
                Command::Close(doc) => self.renderer.close(doc),
                Command::Render { doc, page, width } => self.renderer.render(doc, page, width),
                Command::Want { doc, pages } => self.renderer.want(doc, pages),
                Command::Fetch(path) => {
                    if let Some((host, slot)) = &self.copy {
                        let id = host.pdf_copy(&path, slot);
                        self.surface.fetching(id);
                    }
                }
            }
        }
        self.schedule(cx);
    }

    /// One renderer reply, applied on the foreground outside any window's update, so what it releases leaves every atlas now.
    pub fn apply(&mut self, reply: Reply, cx: &mut Context<Self>) {
        let now = self.now(cx);
        self.surface.reply(reply, now);
        for image in self.surface.cache.take_released() {
            cx.drop_image(image, None);
        }
        self.flush(cx);
        cx.notify();
    }

    /// The host's copy reply for a slow document.
    pub fn copied(
        &mut self,
        id: u64,
        path: &str,
        error: &str,
        stalled: bool,
        cx: &mut Context<Self>,
    ) {
        self.surface.copied(id, path, error, stalled);
        self.flush(cx);
        cx.notify();
    }

    /// The strip for Quick Look's bar; None for the column, which has no strip.
    pub fn bar(&self, i18n: &I18n) -> Option<Bar> {
        let viewer = self.viewer.as_ref()?;
        let (page, count) = (self.surface.page, self.surface.page_count());
        let counter = viewer::counter(self.surface.drawn_page(), count).map(|(drawn, count)| {
            i18n.tr_args(
                "document.pdf_page_count",
                &[&drawn.to_string(), &viewer::count_text(count, self.first)],
            )
        });
        let items = viewer
            .strip(page, count)
            .into_iter()
            .map(|item| (item, item.control.glyph(), i18n.tr(item.control.name_key())))
            .collect();
        Some(Bar { counter, items })
    }

    /// The one sentence a failed document gets, which the host's error recipe draws under its alert mark.
    pub fn fail_sentence(&self, i18n: &I18n) -> Option<String> {
        self.surface
            .failed()
            .then(|| i18n.tr(self.surface.fetch.fail_key()))
    }

    /// PreviewScroll.qml pageBy: move the column stack by its viewport, clamped at either end.
    pub fn page_viewport(&mut self, direction: i64, cx: &mut Context<Self>) {
        let top = self.surface.view_top + direction as f64 * self.surface.view_height;
        self.surface.scroll_to(top);
        self.flush(cx);
        cx.notify();
    }

    /// A turn: the stack scrolls to the page's top, the single page shows the next one.
    pub fn turn(&mut self, delta: i64, cx: &mut Context<Self>) {
        let now = self.now(cx);
        self.surface.turn(delta, now);
        self.flush(cx);
        cx.notify();
    }

    /// One key action from the host's key table, routed through the column's or Quick Look's rules.
    pub fn action(&mut self, action: viewer::Action, cx: &mut Context<Self>) -> viewer::Effect {
        let (page, count) = (self.surface.page, self.surface.page_count());
        let effect = match &mut self.viewer {
            Some(viewer) => viewer.action(action, page, count),
            None => viewer::column_action(action),
        };
        match effect {
            viewer::Effect::Turn(delta) => self.turn(delta, cx),
            viewer::Effect::Scroll(rows) => self.scroll_rows(rows, cx),
            viewer::Effect::Zoom(_) => {
                self.pan = (0., 0.);
                cx.notify();
            }
            _ => cx.notify(),
        }
        effect
    }

    fn scroll_rows(&mut self, rows: i64, cx: &mut Context<Self>) {
        let step = f64::from(self.metrics.row_height);
        if self.surface.continuous {
            let s = &self.surface;
            let top = viewer::scroll_by(s.view_top, rows, step, s.layout.height, s.view_height);
            self.surface.scroll_to(top);
        } else {
            let (_, content) = self.zoomed();
            self.pan.1 = viewer::scroll_by(self.pan.1, rows, step, content.1, self.frame.1);
        }
        self.flush(cx);
        cx.notify();
    }

    fn wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let delta = f64::from(crate::input::scroll::wheel_delta(&event.delta).y);
        if self.surface.continuous {
            let top = self.surface.view_top - delta;
            self.surface.scroll_to(top);
        } else {
            let (_, content) = self.zoomed();
            self.pan.1 = (self.pan.1 - delta).min(content.1 - self.frame.1).max(0.);
        }
        self.flush(cx);
        cx.notify();
    }
}

#[path = "view_paint.rs"]
mod paint;

#[cfg(test)]
#[path = "view_wheel_tests.rs"]
mod wheel_tests;

#[path = "view_scroll.rs"]
mod scroll;
