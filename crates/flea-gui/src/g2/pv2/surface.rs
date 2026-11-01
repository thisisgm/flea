//! One PDF surface's state, the column's continuous stack or Quick Look's single page: which document is open, which pages
//! are wanted at which width, what landed, and what the renderer must do next. Time is milliseconds on the caller's clock.
use super::cache::PageCache;
use super::render::{DocId, Reply};
use flea::ui_logic_pv2::pdf::{self, Cap, Fetch, Layout, Opener, Slot};
use gpui::RenderImage;
use std::{path::PathBuf, sync::Arc, time::Duration};

/// What the surface asks of its renderer and its host.
#[derive(Debug, PartialEq)]
pub enum Command {
    Open(PathBuf),
    Close(DocId),
    Render {
        doc: DocId,
        page: usize,
        width: u32,
    },
    /// The pages still wanted, sent before any render they name; the renderer skips a queued render not among them.
    Want {
        doc: DocId,
        pages: Vec<(usize, u32)>,
    },
    /// The host's slow storage copy for this document, answered through `copied`.
    Fetch(String),
}

/// Counts a test and the native run read: page sizes read, renders asked and landed, the last turn's render time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counters {
    pub size_reads: usize,
    pub asked: usize,
    pub landed: usize,
    pub failed: usize,
    pub last_render: Duration,
}

pub struct Surface {
    pub continuous: bool,
    path: String,
    active: bool,
    pub opener: Opener,
    pub cap: Cap,
    pub fetch: Fetch,
    fetch_first: bool,
    has_backend: bool,
    pub doc: Option<DocId>,
    pub sizes: Vec<(f64, f64)>,
    pub open_failed: bool,
    pub page_failed: bool,
    /// Which page the frame shows, zero-based; in the stack, the page under the frame's top.
    pub page: usize,
    /// The pages' frame in logical pixels: the width pages fill and the viewport's height.
    pub width: f64,
    pub view_height: f64,
    pub scale: f64,
    pub gap: f64,
    pub render_width: u32,
    pub view_top: f64,
    pub layout: Layout,
    pub slots: Vec<Slot>,
    pub cache: PageCache,
    pub inflight: Vec<(usize, u32)>,
    /// Pages of this document that failed to render; never asked again until a switch.
    pub failed_pages: Vec<usize>,
    wanted: Vec<(usize, u32)>,
    /// Quick Look's frame at zoom 1, which the neighbours of a zoomed page render fitted to.
    fitted: (f64, f64),
    pub counters: Counters,
    commands: Vec<Command>,
}

impl Surface {
    pub fn new(continuous: bool, cap_ms: u64, min_show_ms: u64) -> Self {
        Surface {
            continuous,
            path: String::new(),
            active: false,
            opener: Opener::default(),
            cap: Cap::new(cap_ms, min_show_ms),
            fetch: Fetch::InPlace,
            fetch_first: false,
            has_backend: false,
            doc: None,
            sizes: Vec::new(),
            open_failed: false,
            page_failed: false,
            page: 0,
            width: 0.,
            view_height: 0.,
            scale: 1.,
            gap: 0.,
            render_width: 0,
            view_top: 0.,
            layout: Layout::default(),
            slots: Vec::new(),
            cache: PageCache::new(3),
            inflight: Vec::new(),
            failed_pages: Vec::new(),
            wanted: Vec::new(),
            fitted: (0., 0.),
            counters: Counters::default(),
            commands: Vec::new(),
        }
    }

    /// Cancels this surface's document before its retained view leaves the mount.
    pub fn close(&mut self) {
        if let Some(doc) = self.doc.take() {
            self.commands.push(Command::Close(doc));
        }
        self.opener = Opener::default();
        self.active = false;
        self.cache.clear();
        self.inflight.clear();
        self.failed_pages.clear();
        self.wanted.clear();
        self.slots.clear();
        self.cap.reset();
    }

    pub fn take_commands(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }

    /// The page's proportions from the document, 1 while it loads, never from a rendered image.
    pub fn page_aspect(&self) -> f64 {
        self.aspect_of(self.page)
    }

    fn aspect_of(&self, page: usize) -> f64 {
        match self.sizes.get(page.min(self.sizes.len().saturating_sub(1))) {
            Some(&(w, h)) if !self.continuous => pdf::aspect(w, h),
            _ => 1.,
        }
    }

    /// A slow storage class fetches through the host; decided once per opened document.
    pub fn set_fetch(&mut self, fetch_first: bool, has_backend: bool) {
        self.fetch_first = fetch_first;
        self.has_backend = has_backend;
    }

    /// A new document starts at its first page; an idle press opens on the next frame, a walk or a render waits for the settle.
    pub fn set_path(&mut self, path: &str, now: u64) {
        if path != self.path {
            self.page = 0;
        }
        self.path = path.to_string();
        self.opener
            .request(&self.path, self.active, now, self.render_busy());
    }

    pub fn set_active(&mut self, active: bool, now: u64) {
        self.active = active;
        self.opener
            .request(&self.path, self.active, now, self.render_busy());
    }

    /// The next presented frame after a press.
    pub fn frame(&mut self) {
        if self.opener.frame() {
            self.switch();
        }
    }

    /// Every timer: the settle, the cap and the minimum show; true when anything drawn changed.
    pub fn tick(&mut self, now: u64) -> bool {
        let opened = self.opener.tick(now);
        if opened {
            self.switch();
        }
        self.cap.tick(now) || opened
    }

    /// The next deadline any timer holds.
    pub fn next_deadline(&self) -> Option<u64> {
        match (self.opener.settle_at, self.cap.next_deadline()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    // Another document's page is never this one's: it is dropped, and kept by no render, before the switch.
    fn switch(&mut self) {
        if let Some(doc) = self.doc.take() {
            self.commands.push(Command::Close(doc));
        }
        self.cap.reset();
        self.cache.clear();
        self.inflight.clear();
        self.failed_pages.clear();
        self.wanted.clear();
        self.slots.clear();
        self.sizes.clear();
        self.layout = Layout::default();
        self.open_failed = false;
        self.page_failed = false;
        self.view_top = 0.;
        let opened = self.opener.opened.clone();
        self.fetch = Fetch::InPlace;
        if Fetch::decide(&opened, self.fetch_first, self.has_backend) {
            self.fetch = Fetch::Waiting(0);
            self.commands.push(Command::Fetch(opened));
            return;
        }
        self.load();
    }

    /// The host's fetch went out under this id.
    pub fn fetching(&mut self, id: u64) {
        if self.fetch == Fetch::Waiting(0) {
            self.fetch = Fetch::Waiting(id);
        }
    }

    /// A copy reply; only the fetch in flight takes it.
    pub fn copied(&mut self, id: u64, path: &str, error: &str, stalled: bool) {
        if self.fetch.copied(id, path, error, stalled) {
            self.load();
        }
    }

    fn load(&mut self) {
        if let Some(source) = self.fetch.source(&self.opener.opened) {
            self.commands.push(Command::Open(PathBuf::from(source)));
        }
    }

    /// The renderer named the document it opened; the caller ties the id to the open just sent.
    pub fn opening(&mut self, doc: DocId) {
        self.doc = Some(doc);
    }

    /// Quick Look's frame at zoom 1; set before `set_frame` so a zoom change lays out with it.
    pub fn set_fitted(&mut self, width: f64, view_height: f64) {
        self.fitted = (width, view_height);
    }

    /// The frame the pages draw in, in logical pixels, and the window's scale.
    pub fn set_frame(&mut self, width: f64, view_height: f64, scale: f64, gap: f64) {
        let changed = (width, view_height, scale, gap)
            != (self.width, self.view_height, self.scale, self.gap);
        self.width = width;
        self.view_height = view_height;
        self.scale = scale;
        self.gap = gap;
        if changed {
            self.relayout();
        }
    }

    fn relayout(&mut self) {
        if self.continuous {
            let ratios: Vec<f64> = self.sizes.iter().map(|&(w, h)| pdf::ratio(w, h)).collect();
            self.layout = Layout::new(&ratios, self.width, self.gap);
            self.view_top = self
                .view_top
                .min((self.layout.height - self.view_height).max(0.));
        }
        let target = self.target(self.page);
        self.render_width = target;
        self.want();
    }

    /// Applies one renderer reply; an image for a document no longer open is released, never drawn.
    pub fn reply(&mut self, reply: Reply, now: u64) {
        match reply {
            Reply::Opened { doc, sizes, read } if Some(doc) == self.doc => {
                self.counters.size_reads += read;
                self.sizes = sizes;
                self.page = self.page.min(self.sizes.len().saturating_sub(1));
                self.relayout();
            }
            Reply::Sizes { doc, sizes, read } if Some(doc) == self.doc => {
                self.counters.size_reads += read;
                self.sizes = sizes;
                self.relayout();
            }
            Reply::OpenFailed { doc, .. } if Some(doc) == self.doc => self.open_failed = true,
            Reply::Rendered {
                doc,
                page,
                width,
                image,
                took,
            } if Some(doc) == self.doc => {
                self.counters.landed += 1;
                self.counters.last_render = took;
                self.inflight.retain(|&(p, w)| (p, w) != (page, width));
                let keep = self.kept();
                self.cache.insert(page, width, image, &keep);
                self.settle(page, true, now);
            }
            Reply::Rendered { image, .. } => self.cache.release(image),
            Reply::RenderFailed { doc, page, width } if Some(doc) == self.doc => {
                self.counters.failed += 1;
                self.inflight.retain(|&(p, w)| (p, w) != (page, width));
                if !self.failed_pages.contains(&page) {
                    self.failed_pages.push(page);
                }
                self.settle(page, false, now);
            }
            _ => {}
        }
        self.want();
    }

    // A landed or failed page: the single page's cap ends, the stack's slot settles and may hold the first frame.
    fn settle(&mut self, page: usize, landed: bool, now: u64) {
        if !self.continuous {
            if page == self.page {
                self.cap.render_finished(landed.then_some(page), now);
            }
            return;
        }
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.page == page) {
            slot.rendering = self.inflight.iter().any(|&(p, _)| p == page);
            slot.landed |= landed;
            slot.errored = !landed;
        }
        self.first_frame();
    }

    /// A turn by `delta` pages: the stack scrolls to the page's top, the single page shows it from the cache or renders it.
    pub fn turn(&mut self, delta: i64, now: u64) {
        if self.sizes.is_empty() {
            return;
        }
        if self.continuous {
            if let Some(top) = self.layout.turn_offset(self.page, delta, self.view_height) {
                self.scroll_to(top);
            }
            return;
        }
        let next = pdf::clamp_page(self.page, delta, self.sizes.len());
        if next == self.page {
            return;
        }
        let loading = self.render_busy();
        self.page = next;
        self.cap.page_changed(next, loading, now);
        if self.cache.nearest(next, self.target(next)).is_some() {
            self.cache.touch(next);
            self.cap.render_finished(Some(next), now);
        } else if self.failed_pages.contains(&next) {
            self.cap.render_finished(None, now);
        } else {
            self.cap.render_started(next, now);
        }
        self.relayout();
    }

    /// The stack's scroll offset; the page under the frame's top follows it.
    pub fn scroll_to(&mut self, top: f64) {
        if !self.continuous {
            return;
        }
        self.view_top = top.min(self.layout.height - self.view_height).max(0.);
        if !self.sizes.is_empty() {
            self.page = self.layout.page_at(self.view_top);
        }
        self.want();
    }

    /// The image a page draws with: its exact render, or the closest width while that renders.
    pub fn image(&self, page: usize) -> Option<&Arc<RenderImage>> {
        self.cache.nearest(page, self.render_width)
    }

    /// The single page's image while it is the page on screen and no loading state stands in for it.
    pub fn single_image(&self) -> Option<&Arc<RenderImage>> {
        if self.continuous || self.cap.fell_back {
            return None;
        }
        self.cap
            .shown_page
            .and_then(|shown| self.cache.nearest(shown, self.render_width))
    }
}

#[path = "surface_want.rs"]
mod want;

#[cfg(test)]
#[path = "surface_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "surface_fix_tests.rs"]
mod fix_tests;

#[cfg(test)]
#[path = "surface_quality_tests.rs"]
mod preview_quality_tests;
