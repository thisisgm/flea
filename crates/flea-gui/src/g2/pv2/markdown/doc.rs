//! The shown document's state: its read, its blocks and their layouts, and the jobs that lay out what the frame shows first.
use super::assets::{Art, Snapshot};
use super::worker::{Lay, Parsed, Read, Reply};
use super::MarkdownView;
use flea::ui_logic_pv2::markdown::{
    self as md,
    markdown_layout::{self as layout, Counter, Geometry, Metrics},
    Block,
};
use gpui::{Context, RenderImage};
use std::{collections::HashMap, ops::Range, sync::Arc};

/// Blocks kept laid past the frame's edges, so a short scroll lays nothing out first.
pub const CACHE_PX: f32 = 600.;
/// Blocks laid out per background job once the frame's own are done.
pub const BACKGROUND_CHUNK: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Loading,
    Ready,
    TooLarge,
    Unreadable,
}

/// A block's layout and the epoch and asset version it was laid at.
#[derive(Clone, Debug)]
pub struct Laid {
    pub geometry: Arc<Geometry>,
    pub generation: u64,
    pub version: u64,
}

#[derive(Clone)]
pub enum PictureState {
    Asked,
    Drawn(Arc<RenderImage>),
    Failed,
}

/// A fence's colour asks: the next 1-based line to ask, the token in flight and whether it is done.
#[derive(Clone, Debug, Default)]
pub struct FenceAsk {
    pub token: u64,
    pub next: usize,
    pub lines: Vec<Vec<(i64, String)>>,
    pub done: bool,
}

#[derive(Default)]
pub struct Doc {
    pub seq: u64,
    pub status: Status,
    pub text: Option<Arc<str>>,
    pub starts: Arc<Vec<usize>>,
    pub lines: usize,
    pub parsed: Parsed,
    /// The whole parse landed, not only its head.
    pub complete: bool,
    pub laid: Vec<Option<Laid>>,
    pub(super) asked: Vec<(u64, u64)>,
    pub(super) dirty: Vec<u64>,
    pub generation: u64,
    pub width: f32,
    pub assets: Arc<Snapshot>,
    pub version: u64,
    pub tops: Vec<f32>,
    pub height: f32,
    pub(super) tops_stale: bool,
    pub pictures: HashMap<String, PictureState>,
    pub(super) picture_blocks: HashMap<String, Vec<usize>>,
    pub art: HashMap<Art, Arc<RenderImage>>,
    pub(super) figure_tickets: HashMap<u64, usize>,
    pub(super) formula_tickets: HashMap<u64, Vec<usize>>,
    pub(super) formula_sources: HashMap<String, u64>,
    pub(super) formula_boxes: HashMap<usize, (f32, f32, f32)>,
    pub(super) formulas_settled: std::collections::HashSet<usize>,
    pub fences: HashMap<usize, FenceAsk>,
    /// The first screen was drawn; the hold never applies again to this document.
    pub shown: bool,
    pub loaded_at: u64,
    pub(super) released: Vec<Arc<RenderImage>>,
}

impl Doc {
    pub fn blocks(&self) -> &[Block] {
        &self.parsed.blocks
    }

    /// A document nested past the limit parses to the one sentinel.
    pub fn deep(&self) -> bool {
        matches!(self.blocks(), [Block::Deep { .. }])
    }

    /// The file is read and wholly parsed, so the bar may count its lines.
    pub fn content_ready(&self) -> bool {
        self.status == Status::Ready && self.complete
    }

    pub fn counter(&self, i: usize) -> Counter {
        self.parsed.prefix.get(i).copied().unwrap_or_default()
    }

    /// The top block holding an ordinal, read from the running totals.
    pub fn block_of(&self, ordinal: usize, of: impl Fn(&Counter) -> usize) -> Option<usize> {
        let n = self.blocks().len();
        let at = self.parsed.prefix.partition_point(|c| of(c) <= ordinal);
        (at >= 1 && at - 1 < n).then(|| at - 1)
    }

    /// A block's own ordinals of one kind.
    pub fn ordinals(&self, i: usize, of: impl Fn(&Counter) -> usize) -> Range<usize> {
        of(&self.counter(i))..of(&self.counter(i + 1))
    }

    pub fn current(&self, i: usize) -> bool {
        self.laid
            .get(i)
            .and_then(Option::as_ref)
            .is_some_and(|laid| {
                laid.generation == self.generation
                    && laid.version >= self.dirty.get(i).copied().unwrap_or(0)
            })
    }

    /// A figure whose drawing is asked for and not yet answered.
    pub fn figure_asked(&self, ordinal: usize) -> bool {
        self.figure_tickets.values().any(|at| *at == ordinal)
    }

    fn asked_current(&self, i: usize) -> bool {
        self.asked.get(i).is_some_and(|(generation, version)| {
            *generation == self.generation && *version >= self.dirty.get(i).copied().unwrap_or(0)
        })
    }

    /// Takes a parse, keeping the layouts of the blocks the last one shared with it.
    pub fn take(&mut self, parsed: Parsed, complete: bool) {
        let n = parsed.blocks.len();
        let same = self
            .blocks()
            .iter()
            .zip(parsed.blocks.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let same = if self.parsed.prefix.get(..same) == parsed.prefix.get(..same) {
            same
        } else {
            0
        };
        self.laid.truncate(same);
        self.laid.resize(n, None);
        self.asked.truncate(same);
        self.asked.resize(n, (0, 0));
        self.dirty.truncate(same);
        self.dirty.resize(n, 0);
        self.parsed = parsed;
        self.complete = complete;
        self.tops_stale = true;
    }

    /// Marks the blocks an asset change reaches for layout again at a new version.
    pub fn touch(&mut self, blocks: &[usize]) -> &mut Snapshot {
        self.version += 1;
        for &i in blocks {
            if let Some(dirty) = self.dirty.get_mut(i) {
                *dirty = self.version;
            }
        }
        Arc::make_mut(&mut self.assets)
    }

    /// Every block's top inside the document and the document's height, laid heights where known and a line each where not.
    pub fn retop(&mut self, metrics: &Metrics) {
        if !self.tops_stale {
            return;
        }
        let estimate = Metrics::line(metrics.body);
        let heights: Vec<f32> = self
            .laid
            .iter()
            .map(|laid| laid.as_ref().map_or(estimate, |laid| laid.geometry.height))
            .collect();
        let (tops, height) = layout::tops(self.blocks(), &heights, metrics);
        self.tops = tops;
        self.height = height;
        self.tops_stale = false;
    }

    /// The blocks any part of [top, bottom) shows, in document coordinates.
    pub fn window(&self, top: f32, bottom: f32, metrics: &Metrics) -> Range<usize> {
        let estimate = Metrics::line(metrics.body);
        let height = |i: usize| {
            self.laid[i]
                .as_ref()
                .map_or(estimate, |laid| laid.geometry.height)
        };
        let first = self.tops.partition_point(|t| *t < top).saturating_sub(1);
        let mut first = first.min(self.tops.len());
        while first < self.tops.len() && self.tops[first] + height(first) < top {
            first += 1;
        }
        let last = self.tops.partition_point(|t| *t < bottom);
        first..last.max(first)
    }

    pub(super) fn reset(&mut self, seq: u64) {
        let released: Vec<Arc<RenderImage>> = std::mem::take(&mut self.pictures)
            .into_values()
            .filter_map(|state| match state {
                PictureState::Drawn(image) => Some(image),
                _ => None,
            })
            .chain(std::mem::take(&mut self.art).into_values())
            .chain(std::mem::take(&mut self.released))
            .collect();
        *self = Doc {
            seq,
            released,
            ..Doc::default()
        };
    }
}

impl MarkdownView {
    pub(super) fn reply(&mut self, reply: Reply, cx: &mut Context<Self>) {
        self.prepare_answer(&reply);
        match reply {
            Reply::Verified { seq } if seq == self.doc.seq => {
                self.confirming = false;
                self.doc.status = Status::Ready;
                self.resume_prepared(cx);
            }
            Reply::Read { seq, read } if seq == self.doc.seq => match read {
                Read::Large => self.doc.status = Status::TooLarge,
                Read::Failed => self.doc.status = Status::Unreadable,
                Read::Text(text, starts) => {
                    self.doc.lines = md::line_count(&text);
                    self.source.set_text(Arc::clone(&text), Arc::clone(&starts));
                    self.doc.text = Some(text);
                    self.doc.starts = starts;
                }
            },
            Reply::Head { seq, parsed } if seq == self.doc.seq && !self.doc.complete => {
                self.doc.take(parsed, false)
            }
            Reply::Parsed {
                seq,
                parsed,
                bytes,
                off_ui,
            } if seq == self.doc.seq => {
                self.counts.parses += 1;
                self.counts.parsed_bytes += bytes;
                if !off_ui {
                    self.counts.ui_parse_bytes += bytes;
                }
                self.doc.take(parsed, true);
                self.doc.status = Status::Ready;
                self.warm(cx);
            }
            Reply::Laid {
                seq,
                generation,
                version,
                laid,
            } if seq == self.doc.seq && generation == self.doc.generation => {
                self.counts.blocks_laid += laid.len();
                for (i, geometry) in laid {
                    let newer = self.doc.laid.get(i).map(|old| {
                        !old.as_ref().is_some_and(|old| {
                            old.version > version && old.generation == generation
                        })
                    });
                    if newer == Some(true) {
                        self.note_pictures(i, &geometry);
                        self.doc.laid[i] = Some(Laid {
                            geometry,
                            generation,
                            version,
                        });
                        self.doc.tops_stale = true;
                    }
                }
            }
            _ => return,
        }
        self.pump(cx);
        cx.notify();
    }

    /// Lays out what the frame shows and its cache first, then the rest a chunk at a time; asks for what those blocks draw.
    pub(super) fn pump(&mut self, cx: &mut Context<Self>) {
        if !self.active
            || self.doc.blocks().is_empty()
            || self.shown_view() != super::View::Rendered
        {
            self.source_pump(cx);
            self.release_hold(cx);
            return;
        }
        let metrics = self.tokens.metrics();
        let width = (self.frame.0 - 2. * self.tokens.inset_x()).floor();
        if width <= 0. {
            return;
        }
        if width != self.doc.width {
            self.doc.width = width;
            self.doc.generation += 1;
        }
        self.doc.retop(&metrics);
        let top = self.scroll - self.tokens.inset_y();
        let window = self
            .doc
            .window(top - CACHE_PX, top + self.frame.1 + CACHE_PX, &metrics);
        let wanted: Vec<usize> = window
            .clone()
            .filter(|i| !self.doc.asked_current(*i))
            .collect();
        let window_done = window.clone().all(|i| self.doc.current(i));
        if !wanted.is_empty() {
            self.lay(wanted);
        } else if window_done {
            let rest: Vec<usize> = (0..self.doc.blocks().len())
                .filter(|i| !self.doc.asked_current(*i))
                .take(BACKGROUND_CHUNK)
                .collect();
            if !rest.is_empty() {
                self.lay(rest);
            }
        }
        for i in window {
            self.ask_for(i, cx);
        }
        self.release_hold(cx);
    }

    fn lay(&mut self, indices: Vec<usize>) {
        let (generation, version) = (self.doc.generation, self.doc.version);
        for &i in &indices {
            self.doc.asked[i] = (generation, version);
        }
        self.counts.layout_jobs += 1;
        self.counts.largest_job = self.counts.largest_job.max(indices.len());
        if self.counts.first_job == 0 {
            self.counts.first_job = indices.len();
        }
        self.workers.lay(Lay {
            seq: self.doc.seq,
            generation,
            version,
            blocks: Arc::clone(&self.doc.parsed.blocks),
            prefix: Arc::clone(&self.doc.parsed.prefix),
            indices,
            width: self.doc.width,
            metrics: self.tokens.metrics(),
            assets: Arc::clone(&self.doc.assets),
            remote: Arc::clone(&self.words.remote),
        });
    }

    /// The blocks the frame shows now, without the cache.
    pub fn visible(&self) -> Range<usize> {
        let top = self.scroll - self.tokens.inset_y();
        self.doc
            .window(top, top + self.frame.1, &self.tokens.metrics())
    }

    /// The document's full height with its insets, the extent a scroll stays inside.
    pub fn content_height(&self) -> f32 {
        self.doc.height + 2. * self.tokens.inset_y()
    }
}
