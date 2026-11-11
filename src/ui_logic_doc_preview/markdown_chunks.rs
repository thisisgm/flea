//! A long list as consecutive blocks the outer list view draws lazily.

use super::markdown_leaf::LIST_CHUNK_ITEMS;
use super::markdown_lists::{ordered_marker, place, run_step};
use super::markdown_model::{Block, List, RunMarker};
use std::collections::HashMap;

struct OpenRun {
    ordered: bool,
    widest: String,
}

/// At each chunk edge k, the runs still open after k entries, as indices into the runs; a run's widest marker grows over the whole list.
fn open_runs(list: &List, size: usize) -> (HashMap<usize, Vec<usize>>, Vec<OpenRun>) {
    let mut runs: Vec<OpenRun> = Vec::new();
    let mut open: Vec<Option<usize>> = Vec::new();
    let mut previous = None;
    let mut edges = HashMap::new();
    for k in 0..list.items.len() {
        if k % size == 0 {
            edges.insert(k, open.iter().flatten().copied().collect());
        }
        let depth = list.depths.as_ref().map_or(0, |depths| depths[k]);
        let given = match &list.markers {
            Some(markers) => markers[k].clone(),
            None if list.ordered => format!("{}.", list.start + k as u64),
            None => String::new(),
        };
        // The runs open here are the ones the list layout opens, since both step through run_step.
        let (joined, ordered) = run_step(&open, previous, depth, &given, list.ordered, &|r| {
            runs[r].ordered
        });
        let run = joined.unwrap_or_else(|| {
            runs.push(OpenRun {
                ordered,
                widest: String::new(),
            });
            runs.len() - 1
        });
        place(&mut open, depth, run);
        if runs[run].ordered && given.chars().count() > runs[run].widest.chars().count() {
            runs[run].widest = given;
        }
        previous = Some(depth);
    }
    edges.insert(list.items.len(), open.iter().flatten().copied().collect());
    (edges, runs)
}

// A run as the chunk carries it: its kind and the widest marker over the whole list, which fixes its text column.
fn carried(edge: &[usize], runs: &[OpenRun]) -> Vec<RunMarker> {
    edge.iter()
        .map(|run| RunMarker {
            ordered: runs[*run].ordered,
            marker: runs[*run].widest.clone(),
        })
        .collect()
}

fn slice<T: Clone>(field: &Option<Vec<T>>, from: usize, to: usize) -> Option<Vec<T>> {
    field.as_ref().map(|all| all[from..to].to_vec())
}

/// A list of 40 items answers two blocks of 32 and 8, numbering continued through start, last keeping one marker column width.
pub fn chunk_list(list: List) -> Vec<Block> {
    let count = list.items.len();
    if count <= LIST_CHUNK_ITEMS {
        return vec![Block::List(list)];
    }
    let size = LIST_CHUNK_ITEMS;
    let mut last = list.start + count as u64 - 1;
    // Entries of a nested list are not numbered items, so the last number is the last one a top-level marker carries.
    if let (Some(depths), Some(markers)) = (&list.depths, &list.markers) {
        for (depth, marker) in depths.iter().zip(markers) {
            if *depth == 0 && ordered_marker(marker) {
                let digits = marker.bytes().take_while(u8::is_ascii_digit).count();
                last = marker[..digits].parse().unwrap_or(last);
            }
        }
    }
    let (edges, runs) = open_runs(&list, size);
    let mut chunks = Vec::with_capacity(count.div_ceil(size));
    for from in (0..count).step_by(size) {
        let to = (from + size).min(count);
        let mut chunk = List {
            ordered: list.ordered,
            start: list.start + from as u64,
            items: list.items[from..to].to_vec(),
            depths: slice(&list.depths, from, to),
            markers: slice(&list.markers, from, to),
            gaps: slice(&list.gaps, from, to),
            parts: slice(&list.parts, from, to),
            last: Some(last),
            joined: from > 0,
            carry: Some(carried(&edges[&from], &runs)),
            tail: Some(carried(&edges[&to], &runs)),
        };
        // A later chunk's first entry keeps the gap above it that the whole list gave it, a nested list's parent being the entry before.
        if let (true, Some(gaps), Some(chunk_gaps)) = (from > 0, &list.gaps, chunk.gaps.as_mut()) {
            let deeper = list
                .depths
                .as_ref()
                .is_some_and(|depths| depths[from] > depths[from - 1]);
            chunk_gaps[0] = gaps[from] || (deeper && gaps[from - 1]);
        }
        chunks.push(Block::List(chunk));
    }
    chunks
}
