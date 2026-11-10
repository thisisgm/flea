// Recursive events are reconciled into the index before the loop publishes a standing-query answer.
use super::searchsession::Session;
use std::sync::Arc;

impl Session {
    pub(crate) fn apply_watch_token(&mut self, token: usize) -> bool {
        if !self.watch.as_ref().is_some_and(|watch| watch.core.token == token) { return false; }
        self.apply_watch_deltas()
    }

    pub(crate) fn apply_watch_deltas(&mut self) -> bool {
        let Some(core) = self.watch.as_ref().map(|watch| Arc::clone(&watch.core)) else { return false };
        core.flush();
        if core.unmounted() {
            self.unmounted = true;
            self.pending_patches.clear();
            let mut index = self.index.lock().unwrap();
            index.mark_partial("watch_failed");
            self.probes.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
            drop(index);
            self.tags.stop();
            self.watch.take();
            drop(self.handle.take());
            self.announced = true;
            if self.standing.as_ref().is_some_and(|(_, query, _)| query.is_empty()) {
                self.standing_while_walking = true;
            }
            self.sizes.forget();
            self.last_query.clear();
            self.last_matches.clear();
            self.pending_rest = None;
            return true;
        }
        if self.walking() { return false; }
        let before = self.index.lock().unwrap().partial;
        let delta = core.drain();
        let mut changed = delta.failed || delta.overflow;
        if delta.failed { self.index.lock().unwrap().mark_partial("watch_failed"); }
        if delta.overflow { self.index.lock().unwrap().mark_partial("watch_overflow"); }
        let mut reset = Vec::new();
        for rel in delta.resets {
            if reset.iter().any(|parent: &std::path::PathBuf| rel.starts_with(parent)) { continue; }
            core.forget(&rel);
            reset.push(rel);
        }
        for rel in delta.dirs {
            if !reset.iter().any(|parent| rel.starts_with(parent)) { changed |= self.patch(&rel); }
        }
        // A parent may have rediscovered a new tree; reset after it so replacement never duplicates descendants.
        for rel in reset {
            let mut index = self.index.lock().unwrap();
            if index.reset_tree(&self.scope, &rel, self.hidden, &self.body, Some(&core)) {
                let added = self.tags.carry(&index);
                index.release_patch();
                drop(index);
                self.tags.read_added(added, &self.index);
                changed = true;
            }
        }
        changed |= self.index.lock().unwrap().partial != before;
        if changed {
            self.sizes.forget();
            self.last_query.clear();
            self.last_matches.clear();
            self.pending_rest = None;
        }
        // A watch that failed while reconciling is reported by the next queued delta as well.
        changed
    }

    #[cfg(test)]
    pub(crate) fn watch_token(&self) -> usize {
        self.watch.as_ref().expect("local watch").core.token
    }
}

pub(crate) fn on_delta(
    out: &mut impl std::io::Write,
    st: &mut super::state::State,
    tb: &super::state::Tables,
    pool: &super::thumbs::Pool,
    token: usize,
) {
    super::searchremap::freeze(st);
    let mut empty = None;
    let answer = st.search_session.as_mut().and_then(|session| {
        if !session.apply_watch_token(token) { return None; }
        let answer = session.re_answer_now();
        if answer.is_none() {
            empty = session.landing_empty().map(|(gen, n, scanned, ms)| (gen, n, scanned, ms, session.partial_reason()));
        }
        answer
    });
    if let Some(answer) = answer { super::searchreq::emit_session_answer(out, st, tb, pool, &answer); }
    if let Some((gen, n, scanned, ms, reason)) = empty {
        writeln!(out, "{}", super::searchreq::searched_status_line(n, scanned, ms, gen, false, reason)).ok();
        out.flush().ok();
    }
}
