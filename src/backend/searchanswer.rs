// Answer construction is unchanged; test clocks expose the cost of each phase.
use super::*;

impl Session {
    // Scores the index and ranks only the first window; the callers own the generations.
    pub(crate) fn answer(&mut self, query: &str, count: usize) -> Answer {
        let started = Instant::now();
        let terms = crate::backend::searchtags::Tags::terms(query);
        let narrow = self.tags.narrows(&self.last_query, query)
            && !self.walking() && !self.standing_while_walking
            && Matcher::can_narrow(&crate::backend::searchtags::Tags::terms(&self.last_query).text, &terms.text);
        let unread = self.tags.prepare(&terms, &self.index, &self.scope, &self.body, self.walking());
        // Without tag terms the match set is final after pass 1, so only the classes the window reaches get their DP.
        let fuzzy = if terms.tags.is_empty() { Matcher::deferred(&terms.text) } else { Matcher::new(&terms.text) };
        let priors = self.priors;
        let index = self.index.lock().unwrap();
        // Only complete, unchanged extensions with stable matching rules can reuse their previous candidates.
        let candidates: Vec<usize> = if narrow && index.partial.is_none() && index.len() == self.matched_index_len {
            self.last_matches.clone()
        } else {
            (0..index.len()).collect()
        };
        #[cfg(test)]
        let scored = candidates.len();
        #[cfg(test)]
        let phase_started = Instant::now();
        let hits = searchindex::score_cached(&fuzzy, &candidates, &index, &mut self.score_scratch);
        #[cfg(test)]
        { self.counters.phases.score = phase_started.elapsed(); }
        #[cfg(test)]
        { self.counters.rows_scored = scored; }
        // Every match stays addressable for narrowing, whatever the window holds.
        let (all, mut hits, below) = if fuzzy.defer {
            #[cfg(test)]
            let phase_started = Instant::now();
            let all: Vec<usize> = hits.iter().map(|(_, row)| *row).collect();
            #[cfg(test)]
            { self.counters.phases.all = phase_started.elapsed(); }
            #[cfg(test)]
            let phase_started = Instant::now();
            let last = searchindex::boundary(&hits, count);
            let (mut head, below): (Vec<_>, Vec<_>) = hits.into_iter()
                .partition(|hit| last.is_some_and(|last| searchindex::class(hit.0) <= last));
            searchindex::resolve(&fuzzy, &index, &mut self.score_scratch, &mut head);
            #[cfg(test)]
            { self.counters.phases.resolve = phase_started.elapsed(); }
            #[cfg(test)]
            let phase_started = Instant::now();
            crate::backend::searchprior::scale(priors, &self.scope, &self.folder, &index, &mut head, all.len());
            #[cfg(test)]
            { self.counters.phases.prior = phase_started.elapsed(); }
            (all, head, below)
        } else {
            #[cfg(test)]
            let phase_started = Instant::now();
            let mut hits = crate::backend::searchprior::rank(priors, &self.scope, &self.folder, &index, hits);
            #[cfg(test)]
            { self.counters.phases.prior = phase_started.elapsed(); }
            self.tags.keep(&mut hits, &terms);
            #[cfg(test)]
            let phase_started = Instant::now();
            let all = hits.iter().map(|(_, row)| *row).collect();
            #[cfg(test)]
            { self.counters.phases.all = phase_started.elapsed(); }
            (all, hits, Vec::new())
        };
        // Top-K first: the window is selected and ordered, and the rest waits in the session.
        let window = count.min(all.len());
        #[cfg(test)]
        let compares = std::cell::Cell::new(0usize);
        let cmp = |a: &(i32, usize), b: &(i32, usize)| {
            #[cfg(test)]
            compares.set(compares.get() + 1);
            rank_cmp(&index, a, b)
        };
        // Score then path length, the rank order's first two fields, compared as one integer key.
        #[cfg(test)]
        let raw_before = index.reconstructed.load(Ordering::Relaxed);
        #[cfg(test)]
        let phase_started = Instant::now();
        let rest = select_head(&mut hits, window, |hit| (Reverse(hit.0), index.path_len(hit.1)), cmp);
        #[cfg(test)]
        {
            self.counters.phases.select = phase_started.elapsed();
            self.counters.phases.cut = searchindex::cut_elapsed();
            self.counters.phases.select_raw = index.reconstructed.load(Ordering::Relaxed) - raw_before;
        }
        self.pending_rest = Rest::held(rest, below, &terms.text, priors, &self.scope, &self.folder, all.len());
        #[cfg(test)]
        {
            self.counters.rank_compares = compares.get();
        }
        #[cfg(test)]
        let phase_started = Instant::now();
        let rows: Vec<(String, bool)> =
            hits[..window].iter().map(|&(_, row)| (index.path(row), index.is_dir(row))).collect();
        #[cfg(test)]
        { self.counters.phases.rows = phase_started.elapsed(); }
        let total = all.len();
        let indexed = index.len();
        self.matched_index_len = indexed;
        let partial = index.partial;
        let walking = self.walking() || unread;
        drop(index);
        Answer {
            gen: 0,
            all,
            rows,
            total,
            indexed,
            walking,
            partial,
            ms: started.elapsed().as_secs_f64() * 1000.0,
            count,
            scope: self.scope.clone(),
            dev: self.dev,
            writable: self.writable,
        }
    }
}

#[cfg(test)]
#[path = "searchrank_tests.rs"]
mod rank_tests;
