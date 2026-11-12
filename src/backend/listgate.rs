//! The existing remote scan gate, kept separate from listing reply emission.
use super::{Done, Job};
use crate::backend::events::Event;
use crate::backend::iomount::{internal_failure, ListErr, ListOut};
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{channel, SendError, Sender};
use std::sync::Arc;
use std::thread;

pub(super) type Work = Box<dyn FnOnce() + Send>;

// What became of the scan out (reading, answered with its Event::List, or let go for a newer list); worker and loop each try to leave READING and the first decides, so a let-go scan never also answers.
const READING: u8 = 0;
const ANSWERED: u8 = 1;
const LET_GO: u8 = 2;

// What waits for a scan: the job, the requests behind it, and the watch bursts and unmounts seen before the new descriptor is known.
pub struct Gate {
    // The loop's channel: the scan answers on it and its worker hands a late watch descriptor back on it.
    answers: crate::backend::events::Sender<Event>,
    // The one thread every remote scan runs on, started by the first remote list, so no later list pays a thread start.
    pub(super) scanner: Option<Sender<Work>>,
    // The newest scan's fate, shared with its worker.
    fate: Arc<AtomicU8>,
    job: Option<Job>,
    deferred: VecDeque<String>,
    changes: Vec<i32>,
    unmounts: Vec<i32>,
    // A poll's change or a probe's figures for the folder being left, which wait like a burst and are judged against the folder that lands.
    held: Vec<Event>,
    // Descriptors a let-go scan handed back while another scan was out; inotify gives the same one for the same folder.
    late: Vec<i32>,
    closed: bool,
}

impl Gate {
    pub fn new(answers: crate::backend::events::Sender<Event>) -> Gate {
        Gate {
            answers,
            scanner: None,
            fate: Arc::new(AtomicU8::new(ANSWERED)),
            job: None,
            deferred: VecDeque::new(),
            changes: Vec::new(),
            unmounts: Vec::new(),
            held: Vec::new(),
            late: Vec::new(),
            closed: false,
        }
    }

    // True while a scan is out, which is when a request cannot be answered against the listing it will replace.
    pub fn holds(&self) -> bool {
        self.job.is_some()
    }

    // The loop's channel, so an inline scan names the same answer path its worker would.
    pub fn answers(&self) -> crate::backend::events::Sender<Event> {
        self.answers.clone()
    }

    pub fn begin(&mut self, job: Job) {
        self.job = Some(job);
    }

    // Lets a plain list scan of another folder go so a newer list starts at once; false when none is out, it answers a search session, or its answer is already on the way.
    pub fn let_go(&mut self, path: &str) -> bool {
        let plain = self
            .job
            .as_ref()
            .is_some_and(|job| job.session.is_none() && job.path != path);
        if !plain
            || self
                .fate
                .compare_exchange(READING, LET_GO, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
        {
            return false;
        }
        self.job = None;
        // The let-go thread ends with its read, and the next remote list starts a fresh one.
        self.scanner = None;
        true
    }

    // Runs the scan on the kept thread; it answers exactly once, a panic included, or the gate would hold for good.
    pub(super) fn scan(
        &mut self,
        path: String,
        work: impl FnOnce(crate::backend::events::Sender<Event>) -> Result<ListOut, ListErr>
            + Send
            + 'static,
    ) {
        let (answers, loop_tx) = (self.answers.clone(), self.answers.clone());
        let fate = Arc::new(AtomicU8::new(READING));
        self.fate = Arc::clone(&fate);
        let run: Work = Box::new(move || {
            let result = catch_unwind(AssertUnwindSafe(|| work(loop_tx))).unwrap_or_else(|_| {
                Err(ListErr {
                    error: internal_failure("scan", &flea::bytename::view(&path)),
                    mode: 0,
                    watch_wd: -1,
                })
            });
            if fate
                .compare_exchange(READING, ANSWERED, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                let _ = answers.send(Event::List(Done { result }));
                return;
            }
            if let Ok(listed) = &result {
                for &wd in &listed.listing.outline_watches {
                    let _ = answers.send(Event::AbandonWatch(wd));
                }
            }
            // Let go for a newer list: nothing is answered, and the watch this scan armed goes back to the loop.
            let wd = match &result {
                Ok(listed) => listed.watch_wd,
                Err(failed) => failed.watch_wd,
            };
            if wd >= 0 {
                let _ = answers.send(Event::AbandonWatch(wd));
            }
        });
        let scanner = self.scanner.get_or_insert_with(|| {
            let (jobs, queue) = channel::<Work>();
            thread::spawn(move || queue.into_iter().for_each(|run| run()));
            jobs
        });
        // A scanner that is gone leaves this scan to the caller and is started afresh next time, so the gate still answers.
        if let Err(SendError(run)) = scanner.send(run) {
            self.scanner = None;
            run();
        }
    }

    pub fn take(&mut self) -> Option<Job> {
        self.job.take()
    }

    pub fn defer(&mut self, line: String) {
        self.deferred.push_back(line);
    }

    pub fn next(&mut self) -> Option<String> {
        self.deferred.pop_front()
    }

    pub fn note_change(&mut self, wd: i32) {
        self.changes.push(wd);
    }

    pub fn take_changes(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.changes)
    }

    // An unmount waits like a change does, but is replayed one by one, in the order it arrived.
    pub fn note_unmount(&mut self, wd: i32) {
        self.unmounts.push(wd);
    }

    pub fn take_unmounts(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.unmounts)
    }

    pub fn hold(&mut self, event: Event) {
        self.held.push(event);
    }

    pub fn take_held(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.held)
    }

    // A late descriptor comes back at once with the ones that waited, or waits while a scan is out that may have armed the same one.
    pub fn park_late(&mut self, wd: i32) -> Vec<i32> {
        self.late.push(wd);
        self.release_late()
    }

    // The descriptors that waited, once no scan is out.
    pub fn release_late(&mut self) -> Vec<i32> {
        if self.holds() {
            Vec::new()
        } else {
            std::mem::take(&mut self.late)
        }
    }

    pub fn close(&mut self) {
        self.closed = true;
    }

    pub fn closed(&self) -> bool {
        self.closed
    }

    // Nothing outstanding and nothing queued: the point at which a closed stdin can leave.
    pub fn idle(&self) -> bool {
        self.job.is_none() && self.deferred.is_empty()
    }
}
