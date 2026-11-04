// Equal-score slash families use the stable order that delays an ancestor behind its last descendant file.
use super::{rank_cmp, CmpOrdering, Index};
use crate::backend::searchnames::{Seeded, EMPTY};
use std::cmp::Reverse;
use std::collections::HashMap;

type Directories = HashMap<(u32, u32), (i32, usize), Seeded>;

#[derive(Default)]
pub(crate) struct RankOrder {
    anchors: Option<Directories>,
}

impl RankOrder {
    pub(crate) fn new(index: &Index, hits: &[(i32, usize)], slash: bool) -> Self {
        if !slash { return Self::default(); }
        let mut directories = Directories::default();
        for &(score, row) in hits.iter().filter(|hit| index.is_dir(hit.1)) {
            let entry = &index.rows[row];
            directories.insert((entry.parent, entry.name), (score, row));
        }
        if directories.is_empty() { return Self::default(); }
        for &(score, row) in hits.iter().filter(|hit| !index.is_dir(hit.1)) {
            let mut parent = index.rows[row].parent;
            while parent != EMPTY {
                let (name, above) = index.blocks.record(parent);
                if let Some((directory_score, anchor)) = directories.get_mut(&(above, name)) {
                    if *directory_score == score && rank_cmp(index, &(score, *anchor), &(score, row)).is_lt() {
                        *anchor = row;
                    }
                }
                parent = above;
            }
        }
        directories.retain(|_, (_, row)| !index.is_dir(*row));
        Self { anchors: (!directories.is_empty()).then_some(directories) }
    }

    fn anchor(&self, index: &Index, hit: &(i32, usize)) -> usize {
        if !index.is_dir(hit.1) { return hit.1; }
        let entry = &index.rows[hit.1];
        self.anchors.as_ref().and_then(|directories| directories.get(&(entry.parent, entry.name)))
            .filter(|(score, _)| *score == hit.0).map_or(hit.1, |(_, row)| *row)
    }

    pub(crate) fn key(&self, index: &Index, hit: &(i32, usize)) -> (Reverse<i32>, usize) {
        (Reverse(hit.0), index.path_len(self.anchor(index, hit)))
    }

    pub(crate) fn cmp(&self, index: &Index, a: &(i32, usize), b: &(i32, usize)) -> CmpOrdering {
        if self.anchors.is_none() { return rank_cmp(index, a, b); }
        let (left, right) = (self.anchor(index, a), self.anchor(index, b));
        rank_cmp(index, &(a.0, left), &(b.0, right))
            .then_with(|| (left != a.1).cmp(&(right != b.1)))
            .then_with(|| rank_cmp(index, a, b))
    }
}
