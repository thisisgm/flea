//! The Markdown view's own threads: one reads and parses the file, one lays its blocks out, so the UI thread never parses or shapes a document.
use super::assets::Snapshot;
use super::style::Faces;
use flea::ui_logic_pv2::markdown::{
    self as md,
    markdown_flow::{Face, Measure},
    markdown_layout::{self as layout, Asks, Counter, Geometry, Metrics},
    Block,
};
use gpui::{px, FontId, TextSystem};
use std::{
    collections::HashMap,
    io::Read as _,
    path::{Path, PathBuf},
    sync::{mpsc, Arc},
    thread,
};

/// A file bigger than this many bytes is refused rather than read whole.
pub const MAX_BYTES: u64 = 1_048_576;
/// A text longer than this checks its head for depth before it parses, as a deep one parses to the sentinel unread.
pub const DEEP_CHECK_BYTES: usize = 65_536;

/// The host's text system as the layout measures it: each character's shaped advance per face and size, kept once read.
pub struct TextMeasure {
    text: Arc<TextSystem>,
    faces: Faces,
    ids: HashMap<Face, FontId>,
    widths: HashMap<(Face, u32, char), f32>,
    extents: HashMap<(Face, u32), (f32, f32)>,
    tables: table_measure::Runs,
    /// Characters shaped so far, in cached individual glyphs or complete table runs.
    pub shaped: usize,
}

// A long-lived measure holds every character it met; past this many it starts over.
const WIDTHS_KEPT: usize = 65_536;

#[path = "table_measure.rs"]
mod table_measure;

impl TextMeasure {
    pub(super) fn reader(&self) -> Self {
        Self::new(Arc::clone(&self.text), self.faces.clone())
    }

    pub fn new(text: Arc<TextSystem>, faces: Faces) -> Self {
        let tables = table_measure::Runs::new(Arc::clone(&text));
        TextMeasure {
            text,
            faces,
            ids: HashMap::new(),
            widths: HashMap::new(),
            extents: HashMap::new(),
            tables,
            shaped: 0,
        }
    }

    fn id(&mut self, face: Face) -> FontId {
        let (text, faces) = (&self.text, &self.faces);
        *self
            .ids
            .entry(face)
            .or_insert_with(|| text.resolve_font(&faces.font(face)))
    }

    /// The face's x-height at a size, the ex a formula is drawn at.
    pub fn x_height(&mut self, face: Face, size: f32) -> f32 {
        let id = self.id(face);
        f32::from(self.text.x_height(id, px(size)))
    }
}

impl Measure for TextMeasure {
    fn table_width(&mut self, text: &str, face: Face, size: f32) -> f32 {
        let (width, shaped) = self.tables.width(text, face, size, &self.faces);
        if shaped {
            self.shaped += text.chars().count();
        }
        width
    }

    fn width(&mut self, text: &str, face: Face, size: f32) -> f32 {
        let id = self.id(face);
        if self.widths.len() > WIDTHS_KEPT {
            self.widths.clear();
        }
        let mut total = 0.;
        for c in text.chars() {
            let key = (face, size.to_bits(), c);
            total += match self.widths.get(&key) {
                Some(width) => *width,
                None => {
                    self.shaped += 1;
                    let width = f32::from(self.text.layout_width(id, px(size), c));
                    self.widths.insert(key, width);
                    width
                }
            };
        }
        total
    }

    fn extent(&mut self, face: Face, size: f32) -> (f32, f32) {
        let id = self.id(face);
        let text = &self.text;
        *self
            .extents
            .entry((face, size.to_bits()))
            .or_insert_with(|| {
                (
                    f32::from(text.ascent(id, px(size))).abs(),
                    f32::from(text.descent(id, px(size))).abs(),
                )
            })
    }
}

/// What a read came to: refused as too large, failed, or the text and where each of its lines starts.
#[derive(Clone, Debug)]
pub enum Read {
    Large,
    Failed,
    Text(Arc<str>, Arc<Vec<usize>>),
}

/// Where each line of a text starts; a trailing line end opens no line of its own.
pub fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        text.match_indices('\n')
            .map(|(at, _)| at + 1)
            .filter(|at| *at < text.len()),
    );
    starts
}

/// A parse's blocks with each one's first ordinals, the totals last, and what they ask the host for.
#[derive(Clone, Debug, Default)]
pub struct Parsed {
    pub blocks: Arc<Vec<Block>>,
    pub prefix: Arc<Vec<Counter>>,
    pub asks: Arc<Asks>,
}

impl Parsed {
    pub fn of(blocks: Vec<Block>) -> Self {
        let mut asks = Asks::default();
        layout::asks(&blocks, &mut asks);
        Parsed {
            prefix: Arc::new(prefix(&blocks)),
            asks: Arc::new(asks),
            blocks: Arc::new(blocks),
        }
    }
}

/// What a parse is given beside the file: its folder, the chip and link colours and the placeholder sentence.
#[derive(Clone, Debug)]
pub struct Parse {
    pub dir: String,
    pub chrome: String,
    pub ink: String,
    pub words: md::Words,
    /// Hand the first blocks over ahead of the rest, for a document with nothing drawn yet.
    pub head: bool,
    pub max_bytes: u64,
    pub previous: Option<Arc<str>>,
}

pub struct Lay {
    pub seq: u64,
    /// The layout epoch: a new width or new sizes start one, and a job of an older one is dropped.
    pub generation: u64,
    /// The asset version the snapshot holds, which the reply carries back.
    pub version: u64,
    pub blocks: Arc<Vec<Block>>,
    pub prefix: Arc<Vec<Counter>>,
    pub indices: Vec<usize>,
    pub width: f32,
    pub metrics: Metrics,
    pub assets: Arc<Snapshot>,
    pub remote: Arc<str>,
}

pub enum Reply {
    Verified {
        seq: u64,
    },
    Read {
        seq: u64,
        read: Read,
    },
    /// The first blocks of a parse still running.
    Head {
        seq: u64,
        parsed: Parsed,
    },
    /// The whole parse, the bytes it read and whether it ran on the parse thread.
    Parsed {
        seq: u64,
        parsed: Parsed,
        bytes: usize,
        off_ui: bool,
    },
    Laid {
        seq: u64,
        generation: u64,
        version: u64,
        laid: Vec<(usize, Arc<Geometry>)>,
    },
}

/// Each block's first figure, formula and fence ordinals in document order, then the totals.
pub fn prefix(blocks: &[Block]) -> Vec<Counter> {
    let mut at = Counter::default();
    let mut out = Vec::with_capacity(blocks.len() + 1);
    for block in blocks {
        out.push(at);
        let own = layout::count(block);
        at.figure += own.figure;
        at.formula += own.formula;
        at.fence += own.fence;
    }
    out.push(at);
    out
}

/// The two threads and the channel their replies land on.
pub struct Workers {
    parse: mpsc::Sender<(u64, PathBuf, Parse, bool)>,
    lay: mpsc::Sender<Lay>,
    pub replies: async_channel::Receiver<Reply>,
    #[cfg(test)]
    _inert: Option<async_channel::Sender<Reply>>,
}

impl Workers {
    pub fn spawn(text: Arc<TextSystem>, faces: Faces) -> Self {
        let (outbox, replies) = async_channel::unbounded();
        let (parse, parse_inbox) = mpsc::channel();
        let (lay, lay_inbox) = mpsc::channel();
        #[cfg(test)]
        if crate::g2::pv2::render::MARKDOWN_INERT.get() {
            return Workers {
                parse,
                lay,
                replies,
                _inert: Some(outbox),
            };
        }
        let parse_out = outbox.clone();
        let _ = thread::Builder::new()
            .name("flea-md-parse".into())
            .spawn(move || parse_loop(parse_inbox, parse_out));
        let measure = TextMeasure::new(text, faces);
        let _ = thread::Builder::new()
            .name("flea-md-layout".into())
            .spawn(move || lay_loop(lay_inbox, outbox, measure));
        Workers {
            parse,
            lay,
            replies,
            #[cfg(test)]
            _inert: None,
        }
    }

    pub fn load(&self, seq: u64, path: PathBuf, parse: Parse, remote: bool) {
        let _ = self.parse.send((seq, path, parse, remote));
    }

    pub fn lay(&self, job: Lay) {
        let _ = self.lay.send(job);
    }
}

fn read(path: &Path, limit: u64) -> Read {
    let Some(file) = flea::backend::regfile::open_regular(path) else {
        return Read::Failed;
    };
    if file.metadata().is_ok_and(|meta| meta.len() > limit) {
        return Read::Large;
    }
    let mut bytes = Vec::new();
    if file.take(limit + 1).read_to_end(&mut bytes).is_err() {
        return Read::Failed;
    }
    if bytes.len() as u64 > limit {
        return Read::Large;
    }
    let text: Arc<str> = String::from_utf8_lossy(&bytes).into();
    let starts = Arc::new(line_starts(&text));
    Read::Text(text, starts)
}

fn parse_loop(
    inbox: mpsc::Receiver<(u64, PathBuf, Parse, bool)>,
    outbox: async_channel::Sender<Reply>,
) {
    while let Ok(first) = inbox.recv() {
        // Only the newest load matters: one queued behind it is for a file no longer shown.
        let (seq, path, parse, remote) = inbox.try_iter().last().unwrap_or(first);
        let limit = if remote {
            flea::ui_logic_pv2::code_view::REMOTE_GATE
        } else {
            MAX_BYTES
        };
        let read = read(&path, limit.min(parse.max_bytes));
        if matches!(&read, Read::Text(text, _) if parse.previous.as_ref().is_some_and(|previous| previous.as_ref() == text.as_ref()))
        {
            if outbox.send_blocking(Reply::Verified { seq }).is_err() {
                return;
            }
            continue;
        }
        let text = match &read {
            Read::Text(text, _) => Some(Arc::clone(text)),
            _ => None,
        };
        if outbox.send_blocking(Reply::Read { seq, read }).is_err() {
            return;
        }
        let Some(text) = text else { continue };
        let ctx = md::Ctx {
            dir: &parse.dir,
            chrome: &parse.chrome,
            ink: &parse.ink,
            words: &parse.words,
        };
        let deep = text.len() > DEEP_CHECK_BYTES && md::deep_head(&text).0;
        let blocks = if deep {
            md::deep_blocks()
        } else {
            let head = if parse.head { md::HEAD_BLOCKS } else { 0 };
            md::blocks_headed(&text, &ctx, head, &mut |blocks| {
                let _ = outbox.send_blocking(Reply::Head {
                    seq,
                    parsed: Parsed::of(blocks.to_vec()),
                });
            })
        };
        let reply = Reply::Parsed {
            seq,
            parsed: Parsed::of(blocks),
            bytes: if deep { 0 } else { text.len() },
            off_ui: thread::current().name() == Some("flea-md-parse"),
        };
        if outbox.send_blocking(reply).is_err() {
            return;
        }
    }
}

fn lay_loop(
    inbox: mpsc::Receiver<Lay>,
    outbox: async_channel::Sender<Reply>,
    mut measure: TextMeasure,
) {
    while let Ok(first) = inbox.recv() {
        let jobs: Vec<Lay> = std::iter::once(first).chain(inbox.try_iter()).collect();
        // A job of an older document or an older width is dropped unread.
        let newest = jobs
            .iter()
            .map(|job| (job.seq, job.generation))
            .max()
            .unwrap_or_default();
        for job in jobs
            .into_iter()
            .filter(|job| (job.seq, job.generation) == newest)
        {
            let mut laid = Vec::with_capacity(job.indices.len());
            for i in job.indices {
                let Some(block) = job.blocks.get(i) else {
                    continue;
                };
                let mut ctx = layout::Ctx {
                    metrics: job.metrics,
                    measure: &mut measure,
                    assets: job.assets.as_ref(),
                    counter: job.prefix.get(i).copied().unwrap_or_default(),
                    remote: &job.remote,
                };
                laid.push((i, Arc::new(layout::layout(block, job.width, &mut ctx))));
            }
            if outbox
                .send_blocking(Reply::Laid {
                    seq: job.seq,
                    generation: job.generation,
                    version: job.version,
                    laid,
                })
                .is_err()
            {
                return;
            }
        }
    }
}
