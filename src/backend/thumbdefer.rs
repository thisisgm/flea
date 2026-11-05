// The thumbnail jobs the pool has no room for yet, held in request order, so a row the client still wants waits instead of being evicted.
use crate::backend::thumbs::{Job, Pool, MAX_QUEUE};
use crate::backend::proto::thumbed_line;
use crate::backend::state::State;
use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;

// Far above any viewport, so it bounds a request that names nonsense rather than a real screen; past it a row is answered empty.
pub(crate) const MAX_DEFERRED: usize = 4096;

#[derive(Default)]
pub struct Deferred {
    jobs: VecDeque<Job>,
}

impl Deferred {
    pub fn full(&self) -> bool {
        self.jobs.len() >= MAX_DEFERRED
    }

    // Queues behind the jobs already waiting; the request path pumps after mapping the row.
    pub fn enqueue(&mut self, job: Job) {
        self.jobs.push_back(job);
    }

    // A waiting job for this path, taken back by a cancel the way the pool gives up a queued one.
    pub fn take(&mut self, path: &Path, trash_identity: Option<&str>) -> Vec<Job> {
        let at = self.jobs.iter().position(|j| j.path == path && j.trash_identity.as_deref() == trash_identity);
        at.and_then(|i| self.jobs.remove(i)).into_iter().collect()
    }

    // Ordinary cancellation retains Trash jobs in their original request order.
    pub fn take_listing(&mut self) -> Vec<Job> {
        let (dropped, kept): (Vec<Job>, Vec<Job>) = self.jobs.drain(..).partition(|j| j.trash_identity.is_none());
        self.jobs = kept.into();
        dropped
    }

    // Test-only: every waiting job, so a harness can inspect or run a complete batch.
    #[cfg(test)]
    pub fn take_all(&mut self) -> Vec<Job> {
        self.jobs.drain(..).collect()
    }
}

// The room is a snapshot; any job submit returns must be unmapped and answered even in a release build.
pub(crate) fn pump(out: &mut impl Write, st: &mut State, pool: &Pool) {
    submit_waiting(out, st, pool, MAX_QUEUE.saturating_sub(pool.pending()));
}

fn submit_waiting(out: &mut impl Write, st: &mut State, pool: &Pool, mut room: usize) {
    while room > 0 {
        let Some(job) = st.deferred.jobs.pop_front() else { break };
        for dropped in pool.submit(job) {
            st.outstanding = st.outstanding.saturating_sub(1);
            super::trashthumb::answer(out, st, &dropped.path, dropped.mtime, dropped.trash_identity.as_deref(), "");
            let legacy = if let Some(at) = st.asked.iter().position(|(p, _)| dropped.trash_identity.is_none() && *p == dropped.path && st.thumb_mtimes.get(p) == Some(&dropped.mtime)) {
                st.thumb_mtimes.remove(&dropped.path);
                let row = st.asked.remove(at).1;
                writeln!(out, "{}", thumbed_line(row, "", 0.0)).ok();
                true
            } else { false };
            let observed = super::thumbreq::identity::complete(out, st, &dropped.replies, "", 0.0);
            if legacy || observed { out.flush().ok(); }
        }
        room -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testdir::TestDir;
    use crate::backend::events::channel;

    #[test]
    fn a_submit_return_is_answered_and_released_after_the_room_snapshot() {
        let d = TestDir::new("thumbdeferreturned");
        let (events, _) = channel();
        let mut st = State::new(crate::backend::dirsizeworker::Worker::new(events));
        let pool = Pool::idle();
        for row in 0..MAX_QUEUE + 2 {
            let path = d.join(&format!("img-{row}.jpg"));
            st.asked.push((path.clone(), row));
            st.thumb_mtimes.insert(path.clone(), 0);
            st.outstanding += 1;
            let job = Job { replies: None, path, mtime: 0, source_size: None, trash_identity: None, mime: "image/jpeg".to_string(), trace: None };
            if row < MAX_QUEUE {
                assert!(pool.submit(job).is_empty());
            } else {
                st.deferred.enqueue(job);
            }
        }
        // A two-slot snapshot predates a now-full pool, so the real submit returns the two oldest jobs.
        let mut out = Vec::new();
        submit_waiting(&mut out, &mut st, &pool, 2);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.lines().collect::<Vec<_>>(), vec![thumbed_line(0, "", 0.0), thumbed_line(1, "", 0.0)], "each returned job answers empty exactly once");
        assert_eq!(st.outstanding, MAX_QUEUE, "the returned jobs release their accounting");
        assert_eq!(st.asked.len(), MAX_QUEUE, "the returned jobs release their mappings");
        assert!(st.asked.iter().all(|(_, row)| *row >= 2), "neither returned row remains mapped");
        assert_eq!(pool.pending(), MAX_QUEUE, "the submitted jobs stay queued");
        assert!(st.deferred.take_all().is_empty(), "both waiting jobs reached submit");
    }
}

#[cfg(test)]
#[path = "thumbidentity_drop_tests.rs"]
mod observation_tests;
