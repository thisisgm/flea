use super::*;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use crate::backend::searchnames::Seeded;

// The independent oracle uses raw ancestry edges and a stable Kahn traversal, not production anchors.
pub(crate) fn order(index: &Index, pairs: &mut [((i32, usize), i32)]) {
    let mut base = pairs.to_vec();
    base.sort_by(|a, b| rank_cmp(index, &a.0, &b.0));
    let directories: HashMap<Vec<u8>, usize, Seeded> = base.iter().enumerate()
        .filter(|(_, pair)| index.is_dir(pair.0.1)).map(|(at, pair)| (index.raw(pair.0.1), at)).collect();
    let mut remaining = vec![0usize; base.len()];
    let mut successors = vec![Vec::new(); base.len()];
    for (at, pair) in base.iter().enumerate().filter(|(_, pair)| !index.is_dir(pair.0.1)) {
        let raw = index.raw(pair.0.1);
        for slash in raw.iter().enumerate().filter_map(|(at, &byte)| (byte == b'/').then_some(at)) {
            if let Some(&ancestor) = directories.get(&raw[..slash]) {
                if base[ancestor].0.0 == pair.0.0 {
                    remaining[ancestor] += 1;
                    successors[at].push(ancestor);
                }
            }
        }
    }
    let mut ready: BinaryHeap<_> = remaining.iter().enumerate().filter(|(_, count)| **count == 0)
        .map(|(at, _)| Reverse(at)).collect();
    let mut written = 0;
    while let Some(Reverse(at)) = ready.pop() {
        pairs[written] = base[at];
        written += 1;
        for &ancestor in &successors[at] {
            remaining[ancestor] -= 1;
            if remaining[ancestor] == 0 { ready.push(Reverse(ancestor)); }
        }
    }
    assert_eq!(written, base.len(), "file-to-ancestor dependencies must be acyclic");
}
