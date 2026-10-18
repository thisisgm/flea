use super::{dirsize::DirSize, listing::Listing, meta::stat_all, sort::name_order};
use std::{cmp::Ordering, path::Path, time::Instant};

// The TUI keeps folders alphabetic before size-sorted files; folder display sizes stay viewport requests.
pub fn sort(l: &mut Listing, base: &Path, desc: bool, hidden_last: bool) -> (f64, f64, Vec<Option<DirSize>>) {
    let (stats, ms) = stat_all(base, l);
    let start = Instant::now();
    let mut indices: Vec<usize> = (0..l.len()).collect();
    indices.sort_by(|&a, &b| {
        if hidden_last {
            let hidden = super::sort::is_hidden(l.name(a)).cmp(&super::sort::is_hidden(l.name(b)));
            if hidden != Ordering::Equal { return hidden; }
        }
        match (l.is_dir(a), l.is_dir(b)) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (true, true) => name_order(l.name(a).as_bytes(), l.name(b).as_bytes()),
            (false, false) => {
                let order = stats[a].size.cmp(&stats[b].size).then_with(|| name_order(l.name(a).as_bytes(), l.name(b).as_bytes()));
                if desc { order.reverse() } else { order }
            }
        }
    });
    l.spans = indices.into_iter().map(|i| l.spans[i]).collect();
    (ms, start.elapsed().as_secs_f64() * 1000.0, Vec::new())
}

#[cfg(test)]
#[path = "tuifolders_tests.rs"]
mod tests;
