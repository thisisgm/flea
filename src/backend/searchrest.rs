// The matches past the head: the boundary classes' ranked remainder, then lower classes scored only when a scroll reaches them.
use crate::backend::searchindex::{boundary, class, resolve, Index, QueryScratch, RankOrder};
use crate::backend::searchmatch::Matcher;
use crate::backend::searchprior::{scale, Priors};
use std::path::{Path, PathBuf};

pub(crate) struct Rest {
    pub(super) ranked: Vec<(i32, usize)>,
    // Pass-1 or exact scores; every row here ranks below every row in `ranked`.
    below: Vec<(i32, usize)>,
    query: String,
    priors: Option<&'static Priors>,
    scope: PathBuf,
    folder: PathBuf,
    matched: usize,
}

impl Rest {
    pub(super) fn held(ranked: Vec<(i32, usize)>, below: Vec<(i32, usize)>, query: &str, priors: Option<&'static Priors>,
        scope: &Path, folder: &Path, matched: usize) -> Option<Self> {
        (!ranked.is_empty() || !below.is_empty()).then(|| Self {
            ranked, below, query: query.to_string(), priors, scope: scope.to_path_buf(), folder: folder.to_path_buf(), matched,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.ranked.len() + self.below.len()
    }

    // Re-ranking the remaining dependency graph preserves the suffix of its stable total order.
    pub(super) fn take_ranked(&mut self, take: usize, index: &Index) -> Vec<(String, bool)> {
        let order = RankOrder::new(index, &self.ranked, self.query.contains('/'));
        let ranked = &mut self.ranked;
        if take < ranked.len() {
            ranked.select_nth_unstable_by(take - 1, |a, b| order.cmp(index, a, b));
            ranked[..take].sort_by(|a, b| order.cmp(index, a, b));
        } else {
            ranked.sort_by(|a, b| order.cmp(index, a, b));
        }
        let taken: Vec<(i32, usize)> = ranked.drain(..take).collect();
        taken.iter().map(|&(_, row)| (index.path(row), index.is_dir(row))).collect()
    }

    // Whole lower classes join the ranked part, best first, until it holds `need` rows, scored as the answer would have.
    pub(super) fn promote(&mut self, need: usize, index: &Index, scratch: &mut QueryScratch) {
        let Some(last) = boundary(&self.below, need.saturating_sub(self.ranked.len())) else { return };
        let (mut joining, below): (Vec<_>, Vec<_>) = std::mem::take(&mut self.below).into_iter().partition(|hit| class(hit.0) <= last);
        self.below = below;
        scratch.begin(index);
        resolve(&Matcher::new(&self.query), index, scratch, &mut joining);
        scale(self.priors, &self.scope, &self.folder, index, &mut joining, self.matched);
        self.ranked.append(&mut joining);
    }
}
