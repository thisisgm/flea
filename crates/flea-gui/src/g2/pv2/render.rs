//! A surface's own render thread: it opens each document once, reads every page size once, and renders the pages still wanted.
use super::poppler::{Document, Failure};
use gpui::RenderImage;
use std::{
    cell::Cell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// A document as one open of it, so a reply for a document since closed is told from the one open now.
pub type DocId = u64;

/// The pages of one document still wanted, which the thread reads before each render.
type Wanted = Mutex<(DocId, Vec<(usize, u32)>)>;

/// A document whose sizes are read on idle steps: the list so far, the next page to read and the reads its open answered.
struct Pending {
    doc: DocId,
    sizes: Vec<(f64, f64)>,
    from: usize,
    opened: usize,
}

/// Pages a single page reads at its open; the rest are read once the thread is idle.
const FIRST_SIZES: usize = 2;
/// Sizes read per idle step, so a queued render waits for at most one step.
const SIZE_STEP: usize = 256;

pub enum Job {
    Open {
        doc: DocId,
        path: PathBuf,
        continuous: bool,
    },
    Render {
        doc: DocId,
        page: usize,
        width: u32,
    },
    Close {
        doc: DocId,
    },
}

pub enum Reply {
    /// Quick Look's sizes in points, `read` of them read and the rest standing in as the first page's until `Sizes`; a column open answers no sizes and `read` 0.
    Opened {
        doc: DocId,
        sizes: Vec<(f64, f64)>,
        read: usize,
    },
    /// The full size list of a document opened before every size was read, built over idle steps, `read` more of them read.
    Sizes {
        doc: DocId,
        sizes: Vec<(f64, f64)>,
        read: usize,
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

/// The wanted list's document while the renderer drops: no queued open or render runs.
const CLOSING: DocId = DocId::MAX;

#[cfg(test)]
thread_local! {
    // Set while a test builds surfaces it answers by hand, so no reply reaches its scheduler from another thread.
    static INERT: Cell<bool> = const { Cell::new(false) };
    // The same for Markdown's parse, layout and picture workers, for a test that sets their documents itself.
    pub(crate) static MARKDOWN_INERT: Cell<bool> = const { Cell::new(false) };
}

/// Runs `make` with renderers that start no thread, for tests that feed their surfaces every reply themselves.
#[cfg(test)]
pub fn without_threads<R>(make: impl FnOnce() -> R) -> R {
    INERT.set(true);
    let made = make();
    INERT.set(false);
    made
}

/// Runs `make` with Markdown workers that start no thread, for tests that set each document's state by hand.
#[cfg(test)]
pub fn without_markdown_threads<R>(make: impl FnOnce() -> R) -> R {
    MARKDOWN_INERT.set(true);
    let made = make();
    MARKDOWN_INERT.set(false);
    made
}

/// The renderer threads whose surfaces dropped; `reap` joins them once the event loop has returned.
static REAPER: Mutex<Vec<thread::JoinHandle<()>>> = Mutex::new(Vec::new());

/// The handle a surface keeps; dropping it skips the queued work and hands its thread to the reaper, never waiting on the UI thread.
pub struct Renderer {
    jobs: Option<mpsc::Sender<Job>>,
    pub replies: async_channel::Receiver<Reply>,
    next: Cell<DocId>,
    wanted: Arc<Wanted>,
    thread: Option<thread::JoinHandle<()>>,
    // An inert renderer keeps its reply channel open without a thread.
    #[cfg(test)]
    _outbox: Option<async_channel::Sender<Reply>>,
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Ok(mut wanted) = self.wanted.lock() {
            *wanted = (CLOSING, Vec::new());
        }
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            if let Ok(mut reaper) = REAPER.lock() {
                // Each finished handle joins at once, so the list holds only threads still ending their step.
                let old = std::mem::take(&mut *reaper);
                reaper.reserve(old.len() + 1);
                for handle in old {
                    if handle.is_finished() {
                        let _ = handle.join();
                    } else {
                        reaper.push(handle);
                    }
                }
                reaper.push(thread);
            }
        }
    }
}

/// The dropped renderers still waiting for their join; the drop joins every handle already finished, so this holds only threads still ending their step.
pub fn reaper_len() -> usize {
    REAPER.lock().map(|reaper| reaper.len()).unwrap_or(0)
}

/// Joins every renderer thread dropped so far; main calls it once the event loop has returned.
pub fn reap() {
    let threads = REAPER
        .lock()
        .map(|mut reaper| std::mem::take(&mut *reaper))
        .unwrap_or_default();
    for thread in threads {
        let _ = thread.join();
    }
}

impl Renderer {
    pub fn spawn() -> Self {
        let (jobs, inbox) = mpsc::channel();
        let (outbox, replies) = async_channel::unbounded();
        let wanted = Arc::new(Mutex::new((0, Vec::new())));
        #[cfg(test)]
        if INERT.get() {
            return Renderer {
                jobs: None,
                replies,
                next: Cell::new(0),
                wanted,
                thread: None,
                _outbox: Some(outbox),
            };
        }
        let shared = Arc::clone(&wanted);
        let thread = thread::Builder::new()
            .name("flea-pdf".into())
            .spawn(move || work(inbox, outbox, &shared))
            .ok();
        Renderer {
            jobs: Some(jobs),
            replies,
            next: Cell::new(0),
            wanted,
            thread,
            #[cfg(test)]
            _outbox: None,
        }
    }

    /// Opens a document; the column reads no size at the open and every size in later idle steps, Quick Look its first pages' sizes before its first render.
    pub fn open(&self, path: PathBuf, continuous: bool) -> DocId {
        let doc = self.next.get() + 1;
        self.next.set(doc);
        self.send(Job::Open {
            doc,
            path,
            continuous,
        });
        doc
    }

    fn send(&self, job: Job) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }

    /// The pages of `doc` still wanted; a queued render of that document not among them is skipped.
    pub fn want(&self, doc: DocId, pages: Vec<(usize, u32)>) {
        if let Ok(mut wanted) = self.wanted.lock() {
            *wanted = (doc, pages);
        }
    }

    pub fn render(&self, doc: DocId, page: usize, width: u32) {
        self.send(Job::Render { doc, page, width });
    }

    pub fn close(&self, doc: DocId) {
        self.send(Job::Close { doc });
    }
}

// A render is skipped once its document has a wanted list that no longer names it.
fn unwanted(wanted: &Wanted, doc: DocId, page: usize, width: u32) -> bool {
    wanted
        .lock()
        .is_ok_and(|w| w.0 == CLOSING || (w.0 == doc && !w.1.contains(&(page, width))))
}

// A dropping renderer opens nothing more.
fn closing(wanted: &Wanted) -> bool {
    wanted.lock().is_ok_and(|w| w.0 == CLOSING)
}

fn work(inbox: mpsc::Receiver<Job>, outbox: async_channel::Sender<Reply>, wanted: &Wanted) {
    let mut docs: HashMap<DocId, Document> = HashMap::new();
    let mut pending: Vec<Pending> = Vec::new();
    loop {
        if closing(wanted) {
            return;
        }
        let first = if pending.is_empty() {
            match inbox.recv() {
                Ok(job) => job,
                Err(_) => return,
            }
        } else {
            match inbox.try_recv() {
                Ok(job) => job,
                Err(mpsc::TryRecvError::Disconnected) => return,
                Err(mpsc::TryRecvError::Empty) => {
                    if let Some(reply) = read_sizes(&docs, &mut pending) {
                        if outbox.send_blocking(reply).is_err() {
                            return;
                        }
                    }
                    continue;
                }
            }
        };
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
                    pending.retain(|p| p.doc != doc);
                    None
                }
                Job::Open { doc, .. } if closed.contains(&doc) || closing(wanted) => None,
                Job::Open {
                    doc,
                    path,
                    continuous,
                } => Some(match Document::open(&path) {
                    Ok(document) => {
                        let count = document.page_count();
                        let opened = if continuous {
                            0
                        } else {
                            count.min(FIRST_SIZES)
                        };
                        let mut sizes = vec![(0., 0.); count];
                        for (page, size) in sizes.iter_mut().enumerate().take(opened) {
                            *size = document.page_size(page).unwrap_or((0., 0.));
                        }
                        if opened > 0 {
                            let first = sizes[0];
                            sizes.iter_mut().skip(opened).for_each(|size| *size = first);
                        }
                        let reply = if continuous {
                            Vec::new()
                        } else {
                            sizes.clone()
                        };
                        if opened < count {
                            pending.push(Pending {
                                doc,
                                sizes,
                                from: opened,
                                opened,
                            });
                        }
                        docs.insert(doc, document);
                        Reply::Opened {
                            doc,
                            sizes: reply,
                            read: opened,
                        }
                    }
                    Err(failure) => Reply::OpenFailed { doc, failure },
                }),
                Job::Render { doc, .. } if closed.contains(&doc) => None,
                Job::Render { doc, page, width } if unwanted(wanted, doc, page, width) => None,
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

// One idle step of the first pending size list, answering the whole list once its last size is read.
fn read_sizes(docs: &HashMap<DocId, Document>, pending: &mut Vec<Pending>) -> Option<Reply> {
    let Some(document) = docs.get(&pending[0].doc) else {
        pending.remove(0);
        return None;
    };
    let step = &mut pending[0];
    let end = (step.from + SIZE_STEP).min(step.sizes.len());
    for page in step.from..end {
        step.sizes[page] = document.page_size(page).unwrap_or((0., 0.));
    }
    step.from = end;
    if end < step.sizes.len() {
        return None;
    }
    let done = pending.remove(0);
    Some(Reply::Sizes {
        doc: done.doc,
        read: done.sizes.len() - done.opened,
        sizes: done.sizes,
    })
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
