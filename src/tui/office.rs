// Office documents in the TUI preview: one officepdf request per source, the converted PDF handed to the page path.
use flea::bytename::ExactText as _;
use super::{
    graphics::Task,
    icons, job,
    model::Model,
    pane_state::{self, Kind, Mark, State, Unshown},
    pdf,
    wire::{self, Wire},
};
use crate::jsondoc::{self, Json};
use flea::ui_logic_pv2::office::{self as shared, Applied, Office};
use flea::ui_logic_pv2::office_sniff;
use std::{
    fs::File,
    io,
    os::unix::fs::{FileExt, MetadataExt},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const TITLE: &str = "document.office_title";
const AUTHOR: &str = "document.office_author";
// How long a checked identity waits before the next freshness check, which runs on a worker.
const CHECK_EVERY: Duration = Duration::from_millis(250);
type Gate = Option<Arc<AtomicBool>>;
#[path = "office_container.rs"]
mod container;

// Test seam: a held open or sniff read waits for the test's gate, bounded so a failing run still ends.
#[cfg(test)]
fn hold(gate: &Gate) {
    let Some(gate) = gate else { return };
    let until = Instant::now() + Duration::from_secs(20);
    while !gate.load(Ordering::Relaxed) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[cfg(not(test))]
fn hold(_: &Gate) {}

// A file's identity when it was asked for: the held descriptor and its stamp, checked again off the loop before any use.
pub struct Identity {
    file: File,
    path: PathBuf,
    stamp: [i128; 7],
}
impl Identity {
    fn open(path: &Path, gate: &Gate) -> io::Result<Self> {
        hold(gate);
        let file = job::held(path)?;
        let metadata = file.metadata()?;
        if !container::real(&file, path, metadata.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the Office container is malformed",
            ));
        }
        let stamp = stamp(&metadata);
        Ok(Self {
            file,
            path: path.into(),
            stamp,
        })
    }
    fn fresh(&self) -> bool {
        self.file.metadata().is_ok_and(|m| stamp(&m) == self.stamp)
            && std::fs::symlink_metadata(&self.path).is_ok_and(|m| stamp(&m) == self.stamp)
    }
}
fn stamp(m: &std::fs::Metadata) -> [i128; 7] {
    [
        m.dev() as i128,
        m.ino() as i128,
        m.len() as i128,
        m.mtime() as i128,
        m.mtime_nsec() as i128,
        m.ctime() as i128,
        m.ctime_nsec() as i128,
    ]
}

// A task the subject left: cancelled now, and dropped only once its worker has ended, so the loop never joins a stalled one.
trait Retire {
    fn done(&mut self) -> bool;
}
impl<T: Send + 'static> Retire for Task<T> {
    fn done(&mut self) -> bool {
        self.discard_cancelled()
    }
}

// One preview's Office subject: the shared request state, the source it converts and the identities behind it.
#[derive(Default)]
pub struct Preview {
    pub doc: Office,
    subject: Option<(PathBuf, usize)>,
    source: Option<Arc<Identity>>,
    converted: Option<Arc<Identity>>,
    source_open: Option<Task<Identity>>,
    converted_open: Option<Task<Identity>>,
    sniff: Option<Task<bool>>,
    check: Option<Task<bool>>,
    checked: Option<Instant>,
    retired: Vec<Box<dyn Retire>>,
    issued: u64,
    #[cfg(test)]
    pub gate: Gate,
}

// Asks the backend under a new id; a replaced request is asked again under its own id, keeping its one retry.
impl Preview {
    #[cfg(test)]
    fn gate(&self) -> Gate {
        self.gate.clone()
    }
    #[cfg(not(test))]
    fn gate(&self) -> Gate {
        None
    }
    fn send(&mut self, path: &Path, wire: &mut Wire) -> io::Result<()> {
        self.issued += 1;
        let text = path.exact_text();
        wire.send(vec![
            ("c", wire::word("officepdf")),
            ("id", wire::number(self.issued as usize)),
            ("path", wire::word(&text)),
        ])
    }
    fn ask(&mut self, path: &Path, generation: usize) {
        self.doc.open(&path.exact_text(), 0);
        self.subject = Some((path.into(), generation));
        self.capture(path);
    }
    fn capture(&mut self, path: &Path) {
        self.source = None;
        self.converted = None;
        self.retire_all();
        self.source_open = Some(self.opened(path.into()));
    }
    fn reask(&mut self) {
        let Some((path, _)) = self.subject.clone() else {
            return;
        };
        self.doc.asked(0);
        self.capture(&path);
    }
    // Opens an identity on a worker, so a stalled open never holds the input loop.
    fn opened(&self, path: PathBuf) -> Task<Identity> {
        let gate = self.gate();
        Task::start(move |_| Identity::open(&path, &gate).map_err(|e| e.to_string()))
    }
    fn open_converted(&mut self, path: PathBuf) {
        self.converted = None;
        if let Some(task) = self.converted_open.take() {
            self.retire(task);
        }
        self.converted_open = Some(self.opened(path));
    }
    fn start_sniff(&mut self, path: PathBuf) {
        if let Some(task) = self.sniff.take() {
            self.retire(task);
        }
        let gate = self.gate();
        self.sniff = Some(Task::start(move |cancel| sniff(&path, cancel, &gate)));
    }
    // Cancels a task and leaves it to finish; a later pass reaps it once its worker has ended.
    fn retire<T: Send + 'static>(&mut self, task: Task<T>) {
        task.cancel();
        self.retired.push(Box::new(task));
    }
    fn retire_all(&mut self) {
        if let Some(task) = self.source_open.take() {
            self.retire(task);
        }
        if let Some(task) = self.converted_open.take() {
            self.retire(task);
        }
        if let Some(task) = self.sniff.take() {
            self.retire(task);
        }
        if let Some(task) = self.check.take() {
            self.retire(task);
        }
        self.checked = None;
    }
    // Drops every retired task whose worker has ended; a stalled one stays until it ends.
    fn reap(&mut self) {
        self.retired.retain_mut(|task| !task.done());
    }
    // One pass over the held state: takes finished opens and sniffs, sends the request once the source is captured, runs the freshness check when due, and reports a moved source or PDF.
    fn pass(&mut self, wire: &mut Wire) -> io::Result<bool> {
        self.reap();
        if let Some(result) = self.source_open.as_mut().and_then(Task::poll) {
            self.source_open = None;
            match result {
                Ok(identity) => {
                    let path = identity.path.clone();
                    self.source = Some(Arc::new(identity));
                    // Sent only after capture, so a ready can never pair older converted bytes with a newer source.
                    self.send(&path, wire)?;
                    self.doc.asked(self.issued);
                }
                Err(error) => {
                    self.doc.state = shared::State::Failed(shared::Reason::Other);
                    self.doc.diagnostic = error;
                }
            }
        }
        if let Some(result) = self.converted_open.as_mut().and_then(Task::poll) {
            self.converted_open = None;
            match result {
                Ok(identity) => {
                    self.converted = Some(Arc::new(identity));
                    // A page needs a check of both identities, not an earlier source-only check.
                    if let Some(task) = self.check.take() {
                        self.retire(task);
                    }
                    self.checked = None;
                }
                Err(_) => self.doc.undrawable(shared::PDF_UNOPENED),
            }
        }
        if let Some(result) = self.sniff.as_mut().and_then(Task::poll) {
            self.sniff = None;
            self.doc.sniffed(result.unwrap_or(false));
        }
        let mut moved = false;
        if let Some(result) = self.check.as_mut().and_then(Task::poll) {
            self.check = None;
            self.checked = Some(Instant::now());
            moved = result.ok() != Some(true);
        }
        let due = !self.checked.is_some_and(|at| at.elapsed() < CHECK_EVERY);
        if self.check.is_none() && due {
            let watched: Vec<Arc<Identity>> = [&self.source, &self.converted]
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            if !watched.is_empty() {
                self.check = Some(Task::start(move |_| {
                    Ok(watched.iter().all(|identity| identity.fresh()))
                }));
            }
        }
        Ok(moved)
    }
}

// Whether the selected row is an Office document, the one kind the converter takes.
fn office_row(m: &Model) -> bool {
    m.rows
        .get(&m.cursor)
        .is_some_and(|row| pane_state::kind(row) == Kind::Office)
}

// Drops the request on leaving the document: a late reply finds no id to match and draws nothing.
pub fn forget(state: &mut State) {
    let office = &mut state.office;
    office.doc.leave();
    office.doc.pdf = None;
    office.subject = None;
    office.source = None;
    office.converted = None;
    office.retire_all();
    office.reap();
}

// One input-loop pass: asks for a new source, and asks again when the source or its converted PDF moved.
pub fn step(m: &mut Model, wire: &mut Wire, active: bool) -> io::Result<()> {
    let wanted = (active && office_row(m))
        .then(|| pane_state::content(m))
        .flatten();
    let Some(path) = wanted else {
        forget(&mut m.preview_state);
        return Ok(());
    };
    let generation = m.preview_generation;
    let office = &mut m.preview_state.office;
    let moved =
        office.subject.as_ref() != Some(&(path.clone(), generation)) || office.pass(wire)?;
    if moved {
        office.ask(&path, generation);
        m.pdf = None;
        facts(m);
    }
    project(m);
    Ok(())
}

// The backend's officepdf line for the open source, applied to the request in flight only.
pub fn receive(m: &mut Model, value: &Json, _wire: &mut Wire) -> io::Result<()> {
    let Some(line) = shared::parse(&jsondoc::render(value)) else {
        return Ok(());
    };
    let office = &mut m.preview_state.office;
    match office.doc.reply(&line) {
        Applied::Pdf(path) => office.open_converted(flea::bytename::exact(path)),
        Applied::Sniff => {
            let Some((path, _)) = office.subject.clone() else {
                return Ok(());
            };
            office.start_sniff(path);
        }
        Applied::Ask => office.reask(),
        Applied::Stale | Applied::Converting | Applied::Sheet(_) | Applied::Settled => {}
    }
    project(m);
    Ok(())
}

// The head, the zip's end and content.xml's header of the held file, read off the loop; a cancelled sniff stops at its next read.
fn sniff(path: &Path, cancel: &AtomicBool, gate: &Gate) -> Result<bool, String> {
    let file = job::held(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let (locked, _) = office_sniff::sniff(&path.to_string_lossy(), size, &mut |at, len| {
        hold(gate);
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let mut buf = vec![0; len];
        let read = file.read_at(&mut buf, at).ok()?;
        buf.truncate(read);
        Some(buf)
    });
    Ok(locked)
}

// The failure recipe the shared state draws, one sentence and Open With, or nothing for every state that draws a page.
pub fn project(m: &mut Model) {
    let projection = m.preview_state.office.doc.projection();
    m.preview_state.unshown_key = projection.map(|p| p.key);
    m.preview_state.unshown = projection.map(|p| Unshown {
        sentence: m.preview_state.i18n.tr(p.key),
        open_with: p.open_with,
    });
    m.preview_state.unshown_mark = projection.map(|p| Mark {
        glyph: match p.glyph {
            "lock" => icons::LOCK,
            "file" => icons::FILE,
            _ => super::pane::ALERT,
        },
        error: p.error_tone,
    });
}

// The converted page is drawn once its picture bytes exist: page bytes on graphics, or the half-block PNG without them.
pub fn page_drawn(pdf: &pdf::Pdf) -> bool {
    !pdf.bytes.is_empty() || !pdf.png.is_empty()
}

// The converted PDF once both held identities have passed a freshness check; None otherwise.
pub fn ready_pdf(state: &State) -> Option<PathBuf> {
    let office = &state.office;
    let ready = office.subject.is_some() && office.converted.is_some() && office.checked.is_some();
    office.doc.pdf.as_ref().filter(|_| ready).map(flea::bytename::exact)
}

// The count of pages, sheets or slides, then the title and author of the converted PDF as facts rows; a converter that wrote no title or author gives none.
pub fn facts(m: &mut Model) {
    super::preview::words::metadata(m);
}
pub fn append_facts(m: &mut Model) {
    let office = office_row(m);
    let Some(pdf) = m.pdf.as_ref().filter(|_| office) else {
        return;
    };
    let i18n = &m.preview_state.i18n;
    let mut rows = Vec::new();
    if let Some((key, count)) = m.preview_state.office.doc.count(pdf.pages) {
        rows.push((key, pdf.count_words(i18n, count), true));
    }
    if !pdf.title.is_empty() {
        rows.push((TITLE, pdf.title.clone(), false));
    }
    if !pdf.author.is_empty() {
        rows.push((AUTHOR, pdf.author.clone(), false));
    }
    for (key, value, number) in rows {
        super::preview::words::push(m, key, value, number);
    }
}

#[cfg(test)]
#[path = "office_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "office_identity_tests.rs"]
mod identity_tests;
#[cfg(test)]
#[path = "office_page_tests.rs"]
mod page_tests;
#[cfg(test)]
#[path = "office_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "office_invalid_tests.rs"]
mod invalid_tests;
