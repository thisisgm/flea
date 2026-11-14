//! Where each entry of a list block draws, from the parser's depths, markers and gaps.

use super::markdown_model::List;

/// Unordered markers by depth are disc, circle, square, then square for deeper levels.
pub const BULLETS: [&str; 3] = ["\u{2022}", "\u{25e6}", "\u{25aa}"];

/// The bullet an unordered entry at a depth draws.
pub fn bullet_at(depth: usize) -> &'static str {
    BULLETS[depth.min(BULLETS.len() - 1)]
}

/// True for a marker that numbers its item: "3." does, "\u{2022}" and "" do not.
pub fn ordered_marker(marker: &str) -> bool {
    marker.as_bytes().first().is_some_and(u8::is_ascii_digit)
}

/// The open run an entry joins, or None with the kind of the run it opens: a deeper entry opens a list under the item above,
/// and a changed marker kind starts a list beside the old one.
pub fn run_step(
    open: &[Option<usize>],
    previous: Option<usize>,
    depth: usize,
    given: &str,
    list_ordered: bool,
    ordered_of: &dyn Fn(usize) -> bool,
) -> (Option<usize>, bool) {
    let held = open.get(depth).copied().flatten();
    let ordered = if given.is_empty() {
        held.map_or(list_ordered, ordered_of)
    } else {
        ordered_marker(given)
    };
    let deeper = previous.is_none_or(|previous| depth > previous);
    (
        held.filter(|run| !deeper && ordered == ordered_of(*run)),
        ordered,
    )
}

/// Stores the run an entry joined at open[depth], cutting open to that depth.
pub fn place(open: &mut Vec<Option<usize>>, depth: usize, run: usize) {
    open.resize(depth, None);
    open.push(Some(run));
}

/// Where one entry draws: its text column, its marker column width, its marker and whether a gap stands above it.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub x: f32,
    pub w: f32,
    pub marker: String,
    pub gap: bool,
}

struct Run {
    depth: usize,
    ordered: bool,
    width: f32,
    parent: Option<usize>,
    x: f32,
}

/// Items a (depth 0) and b (depth 1) draw at x 0 and at a's marker width plus spacing; advance is the marker font's advance.
pub fn layout(block: &List, advance: &dyn Fn(&str) -> f32, spacing: f32) -> Vec<Cell> {
    let mut runs: Vec<Run> = Vec::new();
    let mut open: Vec<Option<usize>> = Vec::new();
    let mut previous: Option<usize> = None;
    let bullet = advance(BULLETS[0]);
    // A chunk opens under the runs the list had open before it, each at the column the whole list gives it.
    for (c, held) in block.carry.iter().flatten().enumerate() {
        let widest = if held.ordered {
            held.marker.as_str()
        } else {
            bullet_at(c)
        };
        runs.push(Run {
            depth: c,
            ordered: held.ordered,
            width: advance(widest),
            parent: if c > 0 {
                open.get(c - 1).copied().flatten()
            } else {
                None
            },
            x: 0.0,
        });
        open.resize(c, None);
        open.push(Some(runs.len() - 1));
        previous = Some(c);
    }
    let mut out: Vec<(Cell, usize)> = Vec::with_capacity(block.items.len());
    for k in 0..block.items.len() {
        let depth = block.depths.as_ref().map_or(0, |depths| depths[k]);
        let numbered = format!("{}.", block.start + k as u64);
        let given = match &block.markers {
            Some(markers) => markers[k].as_str(),
            None if block.ordered => numbered.as_str(),
            None => BULLETS[0],
        };
        let (joined, ordered) = run_step(&open, previous, depth, given, block.ordered, &|r| {
            runs[r].ordered
        });
        let run = joined.unwrap_or_else(|| {
            let parent = if depth > 0 {
                open.get(depth - 1).copied().flatten()
            } else {
                None
            };
            runs.push(Run {
                depth,
                ordered,
                width: 0.0,
                parent,
                x: 0.0,
            });
            runs.len() - 1
        });
        place(&mut open, depth, run);
        let marker = if given.is_empty() {
            String::new()
        } else if ordered {
            given.to_string()
        } else {
            bullet_at(depth).to_string()
        };
        runs[run].width = runs[run].width.max(advance(&marker));
        // A long list's later chunks keep the first chunk's marker column width, so a chunk's text stays in line with its neighbours.
        if let (0, true, Some(last)) = (depth, ordered, block.last) {
            runs[run].width = runs[run].width.max(advance(&format!("{last}.")));
        }
        // A joined chunk lies flush under its predecessor, so its first entry keeps the gap the whole list gave it.
        let gap = (k > 0 || block.joined)
            && block.gaps.as_ref().is_some_and(|gaps| {
                gaps[k]
                    || (k > 0 && previous.is_none_or(|previous| depth > previous) && gaps[k - 1])
            });
        out.push((
            Cell {
                x: 0.0,
                w: 0.0,
                marker,
                gap,
            },
            run,
        ));
        previous = Some(depth);
    }
    // The runs still open at the chunk's end take the column the whole list gives them, which a later entry may widen.
    for (t, held) in block.tail.iter().flatten().enumerate() {
        if let Some(Some(run)) = open.get(t).copied() {
            if runs[run].ordered == held.ordered {
                let widest = if held.ordered {
                    held.marker.as_str()
                } else {
                    bullet_at(t)
                };
                runs[run].width = runs[run].width.max(advance(widest));
            }
        }
    }
    // Parents come before their children in runs, so a run's own column is known when its children read it.
    for r in 0..runs.len() {
        runs[r].x = match runs[r].parent {
            Some(parent) => runs[parent].x + runs[parent].width + spacing,
            None => runs[r].depth as f32 * (bullet + spacing),
        };
    }
    out.into_iter()
        .map(|(cell, run)| Cell {
            x: runs[run].x,
            w: runs[run].width,
            ..cell
        })
        .collect()
}

#[cfg(test)]
#[path = "markdown_lists_tests.rs"]
mod tests;
