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
        let narrowed = narrow && index.partial.is_none() && index.len() == self.matched_index_len;
        let candidates: Vec<usize> = if narrowed { self.last_matches.clone() } else { (0..index.len()).collect() };
        #[cfg(test)]
        let mut scored = candidates.len();
        #[cfg(test)]
        let phase_started = Instant::now();
        let (mut hits, typos) = if fuzzy.defer {
            match searchindex::score_shown(&fuzzy, &candidates, &index, &mut self.score_scratch) {
                // Matches that left out their typo rows hold none to narrow to, so an answer that shows typos scores the index.
                (_, true) if narrowed && !self.last_typos => {
                    let every: Vec<usize> = (0..index.len()).collect();
                    #[cfg(test)]
                    { scored += every.len(); }
                    searchindex::score_shown(&fuzzy, &every, &index, &mut self.score_scratch)
                }
                shown => shown,
            }
        } else {
            (searchindex::score_cached(&fuzzy, &candidates, &index, &mut self.score_scratch), true)
        };
        #[cfg(test)]
        { self.counters.phases.score = phase_started.elapsed(); }
        #[cfg(test)]
        { self.counters.rows_scored = scored; }
        // Every match stays addressable for narrowing, whatever the window holds, hidden typo rows included.
        let (all, mut hits, below, hidden) = if fuzzy.defer {
            #[cfg(test)]
            let phase_started = Instant::now();
            let all: Vec<usize> = hits.iter().map(|(_, row)| *row).collect();
            let mut counts = searchindex::class_counts(&hits);
            let hidden = searchindex::hide_typos(&mut hits, &mut counts, fuzzy.budget);
            #[cfg(test)]
            { self.counters.phases.all = phase_started.elapsed(); }
            #[cfg(test)]
            let phase_started = Instant::now();
            let last = searchindex::last_class(&counts, count);
            let (mut head, mut below): (Vec<_>, Vec<_>) = hits.into_iter()
                .partition(|hit| last.is_some_and(|last| searchindex::class(hit.0) <= last));
            below.append(&mut searchindex::resolve_head(&fuzzy, &index, &mut self.score_scratch, &mut head, count));
            #[cfg(test)]
            { self.counters.phases.resolve = phase_started.elapsed(); }
            #[cfg(test)]
            let phase_started = Instant::now();
            crate::backend::searchprior::scale(priors, &self.scope, &self.folder, &index, &mut head, all.len());
            #[cfg(test)]
            { self.counters.phases.prior = phase_started.elapsed(); }
            (all, head, below, hidden)
        } else {
            // Tags filter rows before the typo rule reads them; priors still count every match, as when they ranked first.
            let matched = hits.len();
            self.tags.keep(&mut hits, &terms);
            #[cfg(test)]
            let phase_started = Instant::now();
            let all: Vec<usize> = hits.iter().map(|(_, row)| *row).collect();
            #[cfg(test)]
            { self.counters.phases.all = phase_started.elapsed(); }
            let mut counts = searchindex::class_counts(&hits);
            let hidden = searchindex::hide_typos(&mut hits, &mut counts, fuzzy.budget);
            #[cfg(test)]
            let phase_started = Instant::now();
            crate::backend::searchprior::scale(priors, &self.scope, &self.folder, &index, &mut hits, matched);
            #[cfg(test)]
            { self.counters.phases.prior = phase_started.elapsed(); }
            (all, hits, Vec::new(), hidden)
        };
        let shown = all.len() - hidden;
        // Top-K first: the window is selected and ordered, and the rest waits in the session.
        let window = count.min(shown);
        #[cfg(test)]
        let compares = std::cell::Cell::new(0usize);
        let order = searchindex::RankOrder::new(&index, &hits, terms.text.contains('/'));
        let cmp = |a: &(i32, usize), b: &(i32, usize)| {
            #[cfg(test)]
            compares.set(compares.get() + 1);
            order.cmp(&index, a, b)
        };
        // The score and anchor length lead the shared head/rest order.
        #[cfg(test)]
        let raw_before = index.reconstructed.load(Ordering::Relaxed);
        #[cfg(test)]
        let phase_started = Instant::now();
        let rest = select_head(&mut hits, window, |hit| order.key(&index, hit).1, cmp);
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
        let total = shown;
        let indexed = index.len();
        self.matched_index_len = indexed;
        let partial = index.partial;
        let walking = self.walking() || unread;
        drop(index);
        Answer {
            gen: 0,
            all,
            typos,
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
