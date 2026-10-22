//! A surface's own render thread: it opens each document once, reads every page size once, and renders the pages asked for.
use super::poppler::{Document, Failure};
use gpui::RenderImage;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

/// A document as one open of it, so a reply for a document since closed is told from the one open now.
pub type DocId = u64;

pub enum Job {
    Open { doc: DocId, path: PathBuf },
    Render { doc: DocId, page: usize, width: u32 },
    Close { doc: DocId },
}

pub enum Reply {
    /// Every page's size in points, read once when the document opened.
    Opened {
        doc: DocId,
        sizes: Vec<(f64, f64)>,
    },
    OpenFailed {
        doc: DocId,
        failure: Failure,
    },
    Rendered {
        doc: DocId,
        page: usize,
        width: u32,
        image: Arc<RenderImage>,
        took: Duration,
    },
    RenderFailed {
        doc: DocId,
        page: usize,
        width: u32,
    },
}

/// The handle a surface keeps; dropping it ends the thread once the jobs in hand are done.
pub struct Renderer {
    jobs: mpsc::Sender<Job>,
    pub replies: async_channel::Receiver<Reply>,
    next: Cell<DocId>,
}

impl Renderer {
    pub fn spawn() -> Self {
        let (jobs, inbox) = mpsc::channel();
        let (outbox, replies) = async_channel::unbounded();
        let _ = thread::Builder::new()
            .name("flea-pdf".into())
            .spawn(move || work(inbox, outbox));
        Renderer {
            jobs,
            replies,
            next: Cell::new(0),
        }
    }

    pub fn open(&self, path: PathBuf) -> DocId {
        let doc = self.next.get() + 1;
        self.next.set(doc);
        let _ = self.jobs.send(Job::Open { doc, path });
        doc
    }

    pub fn render(&self, doc: DocId, page: usize, width: u32) {
        let _ = self.jobs.send(Job::Render { doc, page, width });
    }

    pub fn close(&self, doc: DocId) {
        let _ = self.jobs.send(Job::Close { doc });
    }
}

fn work(inbox: mpsc::Receiver<Job>, outbox: async_channel::Sender<Reply>) {
    let mut docs: HashMap<DocId, Document> = HashMap::new();
    while let Ok(first) = inbox.recv() {
        // Everything queued behind the first job is read at once, so a closed document's work is skipped, never done.
        let batch: Vec<Job> = std::iter::once(first).chain(inbox.try_iter()).collect();
        let closed: HashSet<DocId> = batch
            .iter()
            .filter_map(|job| match job {
                Job::Close { doc } => Some(*doc),
                _ => None,
            })
            .collect();
        let mut done: HashSet<(DocId, usize, u32)> = HashSet::new();
        for job in batch {
            let reply = match job {
                Job::Close { doc } => {
                    docs.remove(&doc);
                    None
                }
                Job::Open { doc, .. } if closed.contains(&doc) => None,
                Job::Open { doc, path } => Some(match Document::open(&path) {
                    Ok(document) => {
                        let sizes = (0..document.page_count())
                            .map(|page| document.page_size(page).unwrap_or((0., 0.)))
                            .collect();
                        docs.insert(doc, document);
                        Reply::Opened { doc, sizes }
                    }
                    Err(failure) => Reply::OpenFailed { doc, failure },
                }),
                Job::Render { doc, .. } if closed.contains(&doc) => None,
                Job::Render { doc, page, width } => {
                    if !done.insert((doc, page, width)) {
                        continue;
                    }
                    docs.get(&doc)
                        .map(|document| render(document, doc, page, width))
                }
            };
            if let Some(reply) = reply {
                if outbox.send_blocking(reply).is_err() {
                    return;
                }
            }
        }
    }
}

fn render(document: &Document, doc: DocId, page: usize, width: u32) -> Reply {
    let started = Instant::now();
    let image = document.render(page, width).and_then(|raster| {
        let buffer = image::RgbaImage::from_raw(raster.width, raster.height, raster.bgra)?;
        Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
    });
    match image {
        Some(image) => Reply::Rendered {
            doc,
            page,
            width,
            image,
            took: started.elapsed(),
        },
        None => Reply::RenderFailed { doc, page, width },
    }
}
