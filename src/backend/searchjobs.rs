// One retained execution worker owns the session during an answer; the loop alone publishes it.
use flea::bytename::ExactText as _;
use super::events::Event;
use super::searchreq::{Answer, Cancelled};
use super::searchsession::Session;
use super::state::{State, Tables};
use super::thumbs::Pool;
use crate::error::FleaError;
use std::collections::VecDeque;
use std::io::Write;
use crate::backend::events::Sender;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{channel, sync_channel, Receiver, SyncSender};
use std::thread::JoinHandle;

pub(crate) enum Work {
    Query(String, usize, usize, Option<usize>),
    Walked,
    Held,
    Refresh,
    Delta(usize),
    Cancel,
}

// The test query that mutates the locked index and panics inside Session::ask.
#[cfg(test)]
pub(crate) const INJECTED_PANIC_QUERY: &str = "injected-worker-panic";

struct Done {
    session: Session,
    answer: Option<Answer>,
    cancelled: Option<Cancelled>,
    empty: Option<(usize, usize, usize, f64)>,
    failed: Option<FleaError>,
}

struct Worker {
    tx: SyncSender<(Session, Work)>,
    rx: Receiver<Done>,
    handle: JoinHandle<()>,
}

impl Worker {
    fn new(events: Sender<Event>) -> Self {
        let (tx, jobs) = sync_channel::<(Session, Work)>(1);
        let (done, rx) = channel();
        let handle = std::thread::spawn(move || {
            while let Ok((session, work)) = jobs.recv() {
                if done.send(execute(session, work, &events)).is_err() { break; }
                if events.send(Event::SearchReady).is_err() { break; }
            }
        });
        Self { tx, rx, handle }
    }
}

// One piece of work on the session, on the worker or, for an answer bounded by its few narrowed matches, on the loop.
fn execute(mut session: Session, work: Work, events: &Sender<Event>) -> Done {
    let mut cancelled = None;
    let mut empty = None;
    let ran = catch_unwind(AssertUnwindSafe(|| match work {
        Work::Query(query, gen, count, sizes) => {
            session.wish_sizes(sizes);
            session.ask(gen, &query, count, events)
        }
        Work::Walked => {
            if session.poll_walked() {
                let answer = session.answer_held().or_else(|| session.re_answer_standing());
                if answer.is_none() { empty = session.landing_empty(); }
                answer
            } else { None }
        }
        Work::Held => {
            if session.hold_until.is_some_and(|until| until <= std::time::Instant::now()) {
                session.answer_held()
            } else { None }
        }
        Work::Refresh => session.re_answer_now(),
        // A recursive watch delta is reconciled before its standing query is re-run, as on the loop before.
        Work::Delta(token) => {
            if session.apply_watch_token(token) {
                let answer = session.re_answer_now();
                if answer.is_none() {
                    empty = session.landing_empty();
                }
                answer
            } else {
                None
            }
        }
        Work::Cancel => {
            cancelled = session.cancel_walk();
            None
        }
    }));
    // A panic answers nothing, but Done and SearchReady still go out, so the loop's gate frees and says why.
    let (answer, failed) = match ran {
        Ok(answer) => (answer, None),
        Err(_) => {
            let error = super::iomount::internal_failure("search", &session.scope);
            let (scope, folder, hidden) = (
                session.scope.clone(),
                session.folder.clone(),
                session.hidden,
            );
            session
                .probes
                .cancel
                .store(true, std::sync::atomic::Ordering::Relaxed);
            // A poisoned watch can panic on drop too; none of the failed session's state is reused.
            let _ = catch_unwind(AssertUnwindSafe(|| drop(session)));
            session = Session::open_async(
                &scope.exact_text(),
                &folder.exact_text(),
                hidden,
                events.clone(),
            );
            cancelled = None;
            empty = None;
            (None, Some(error))
        }
    };
    Done { session, answer, cancelled, empty, failed }
}

struct Previous {
    rest: Option<crate::backend::searchsession::Rest>,
    standing: Option<(usize, String, usize)>,
    walking: bool,
    gen: usize,
    epoch: u64,
    announced: bool,
}

#[derive(Default)]
pub(crate) struct Gate {
    worker: Option<Worker>,
    previous: Option<Previous>,
    waiting: VecDeque<Event>,
    discard: bool,
    gen: usize,
}

impl Gate {
    pub(crate) fn busy(&self) -> bool { self.previous.is_some() }

    pub(crate) fn supersede(&mut self, gen: usize) {
        if self.busy() && gen > self.gen { self.discard = true; }
    }

    pub(crate) fn close_after_waiting(&mut self) { self.waiting.push_back(Event::Closed); }

    fn stop(&mut self) {
        if let Some(Worker { tx, rx, handle }) = self.worker.take() {
            drop(tx);
            drop(rx);
            let _ = handle.join();
        }
    }

    // Ordinary events keep their arrival order while a query runs, including windows and scope changes.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn route(&mut self, event: Event, st: &mut State, out: &mut impl Write, tb: &Tables,
        pool: &Pool, events: &Sender<Event>, buffered: &mut VecDeque<Event>, listing_busy: bool) -> Option<Event> {
        if matches!(event, Event::SearchReady) {
            self.finish(st, out, tb, pool, buffered);
            return None;
        }
        if self.busy() {
            if let Event::Request(line) = &event {
                let command = crate::json::field_str(line, "c").unwrap_or_default();
                let newer = command == "searchquery" && crate::json::field_usize(line, "gen").unwrap_or(0) > self.gen;
                // A sort replays after the query in flight, so that query publishes first and is never dropped.
                if newer || matches!(command.as_str(), "searchclose" | "searchopen" | "list" | "listpaths" | "search" | "quit") {
                    self.discard = true;
                }
            }
            self.waiting.push_back(event);
            return None;
        }
        let work = if !listing_busy && st.search_session.is_some() {
            match &event {
                Event::Request(line) => match super::proto::parse_request(line) {
                    super::proto::Request::SearchQuery { query, gen, count, sizes } if !query.is_empty() => {
                        Some(Work::Query(query, gen, count.unwrap_or(super::searchsession::FIRST_WINDOW_DEFAULT), sizes))
                    }
                    super::proto::Request::SearchCancel => Some(Work::Cancel),
                    _ => None,
                },
                Event::SearchWalked => Some(Work::Walked),
                Event::SearchHeld => Some(Work::Held),
                Event::SearchRefresh => Some(Work::Refresh),
                Event::SearchDelta(token) => Some(Work::Delta(*token)),
                _ => None,
            }
        } else { None };
        if let Some(work) = work {
            let mut session = st.search_session.take().unwrap();
            self.gen = match &work { Work::Query(_, gen, _, _) => *gen, _ => session.last_answered_gen };
            // Row ids move only with a patch or a compaction, and both advance the index epoch.
            let epoch = session.index.lock().unwrap().epoch;
            self.previous = Some(Previous {
                rest: session.pending_rest.take(),
                standing: session.standing.clone(),
                walking: session.standing_while_walking,
                gen: session.last_answered_gen,
                epoch,
                announced: session.announced,
            });
            self.discard = false;
            // A few narrowed matches answer sooner on the loop than a worker hop and a doorbell wake take.
            if matches!(&work, Work::Query(query, ..) if session.answers_small(query)) {
                let done = execute(session, work, events);
                self.publish(done, st, out, tb, pool, buffered);
                return None;
            }
            let worker = self.worker.get_or_insert_with(|| Worker::new(events.clone()));
            worker.tx.send((session, work)).unwrap_or_else(|_| panic!("search execution worker stopped"));
            None
        } else {
            if let Event::Request(line) = &event {
                if matches!(crate::json::field_str(line, "c").as_deref(), Some("searchclose" | "searchopen" | "list" | "listpaths" | "sort" | "search" | "quit")) {
                    self.stop();
                }
            }
            Some(event)
        }
    }

    fn finish(&mut self, st: &mut State, out: &mut impl Write, tb: &Tables, pool: &Pool, buffered: &mut VecDeque<Event>) {
        if !self.busy() { return; }
        let done = self.worker.as_ref().unwrap().rx.recv().expect("search result before its wake");
        self.publish(done, st, out, tb, pool, buffered);
    }

    fn publish(&mut self, mut done: Done, st: &mut State, out: &mut impl Write, tb: &Tables, pool: &Pool, buffered: &mut VecDeque<Event>) {
        let previous = self.previous.take().unwrap();
        // Pending patches, a landing's watch deltas and a delta job all renumber through the same epoch.
        let rest_ids_valid = done.failed.is_none() && done.session.index.lock().unwrap().epoch == previous.epoch;
        if done.failed.is_some() {
            // Only the accepted generation is trustworthy after rebuilding; old row ids and queries are gone.
            done.session.last_answered_gen = previous.gen;
        } else if self.discard {
            done.session.pending_rest = if rest_ids_valid { previous.rest } else { None };
            done.session.standing = previous.standing;
            done.session.standing_while_walking = previous.walking;
            done.session.last_answered_gen = previous.gen;
            done.session.last_query.clear();
            done.session.last_matches.clear();
            // Only a landing not yet announced owes the standing generation an answer after replay.
            done.session.announced = previous.announced;
        } else if done.answer.is_none() && done.cancelled.is_none() {
            done.session.pending_rest = if rest_ids_valid { previous.rest } else { None };
        }
        st.search_session = Some(done.session);
        if !self.discard {
            if let Some(answer) = done.answer {
                super::searchreq::emit_session_answer(out, st, tb, pool, &answer);
            }
            if let Some(cancelled) = done.cancelled {
                if let Some(answer) = cancelled.answer {
                    super::searchreq::emit_session_answer_cancelled(out, st, tb, pool, &answer, true);
                } else {
                    super::run::say(out, &super::searchreq::searched_cancelled_line(st.listing.len(), cancelled.indexed, 0.0, cancelled.gen));
                }
            }
            if let Some(error) = done.failed {
                super::run::say(out, &super::proto::error_line(&error));
            }
            if let Some((gen, n, scanned, ms)) = done.empty {
                let reason = st.search_session.as_ref().and_then(Session::partial_reason);
                super::run::say(out, &super::searchreq::searched_status_line(n, scanned, ms, gen, false, reason));
            }
        }
        self.waiting.append(buffered);
        std::mem::swap(buffered, &mut self.waiting);
        if self.discard && !previous.announced { buffered.push_back(Event::SearchWalked); }
    }
}

// A refresh that reaches the loop's dispatch enters the execution gate, or queues behind the answer out; with no session there is nothing to re-run.
pub(crate) fn route_refresh(jobs: Option<&mut Gate>, st: &mut State, out: &mut impl Write, tb: &Tables, pool: &Pool, events: &Sender<Event>) {
    if let Some(jobs) = jobs {
        jobs.route(Event::SearchRefresh, st, out, tb, pool, events, &mut VecDeque::new(), false);
    }
}

// A delta that reaches the loop's dispatch enters the execution gate while it owns or can take the session, else the loop reconciles it.
pub(crate) fn route_delta(
    jobs: Option<&mut Gate>,
    token: usize,
    st: &mut State,
    out: &mut impl Write,
    tb: &Tables,
    pool: &Pool,
    events: &Sender<Event>,
) {
    let routed = match jobs {
        // The dispatch arm before this one holds a delta while a listing scan is out, so here the listing is never busy.
        Some(jobs) if jobs.busy() || st.search_session.is_some() => jobs.route(
            Event::SearchDelta(token),
            st,
            out,
            tb,
            pool,
            events,
            &mut VecDeque::new(),
            false,
        ),
        _ => Some(Event::SearchDelta(token)),
    };
    if let Some(Event::SearchDelta(token)) = routed {
        super::searchdelta::on_delta(out, st, tb, pool, token);
    }
}

impl Drop for Gate {
    fn drop(&mut self) { self.stop(); }
}

#[cfg(test)]
#[path = "searchjobs_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "searchjobs_delta_tests.rs"]
mod delta_tests;

#[cfg(test)]
#[path = "searchjobs_fix_tests.rs"]
mod fix_tests;

#[cfg(test)]
#[path = "searchjobs_inline_tests.rs"]
mod inline_tests;
