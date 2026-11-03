//! What a query leaves standing over a listing, and the runs a row washes.
use super::matcher::{Fit, Matcher, Run};

/// Consecutive positions folded into the stretches a row washes.
pub fn runs_of(positions: &[usize]) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for &at in positions {
        match runs.last_mut() {
            Some(run) if run.start + run.len == at => run.len += 1,
            _ => runs.push(Run { start: at, len: 1 }),
        }
    }
    runs
}

/// ui/js/Fuzzy.js run(): the longest stretch of consecutive positions, the first on a tie.
pub fn longest_run(positions: &[usize]) -> Option<Run> {
    runs_of(positions).into_iter().fold(None, |best: Option<Run>, run| match best {
        Some(kept) if kept.len >= run.len => Some(kept),
        _ => Some(run),
    })
}

/// The rows a query leaves standing: exact matches in listing order, or when there are none the close ones ranked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Narrowed {
    pub exact: Vec<usize>,
    pub close: Vec<usize>,
}

/// None for an empty query; `rows` is walked once, and only the names whose letters fit a close match are weighed after it.
pub fn narrow<'a, I>(query: &str, rows: I) -> Option<Narrowed>
where
    I: IntoIterator<Item = (usize, &'a str)>,
{
    let mut matcher = Matcher::new(query)?;
    let (mut exact, mut fitting): (Vec<usize>, Vec<(usize, &'a str, Fit)>) = (Vec::new(), Vec::new());
    for (row, name) in rows {
        if !exact.is_empty() {
            if matcher.holds(name) {
                exact.push(row);
            }
            continue;
        }
        // Until the first exact match one read screens a name, and only a name that can hold the query is searched for it.
        let fit = matcher.screen(name);
        if matcher.may_hold(fit) && matcher.holds(name) {
            exact.push(row);
        } else if fit != Fit::No {
            fitting.push((row, name, fit));
        }
    }
    if !exact.is_empty() {
        return Some(Narrowed { exact, close: Vec::new() });
    }
    let mut ranked: Vec<(u8, i32, usize)> =
        fitting.into_iter().filter_map(|(row, name, fit)| matcher.rank_fitting(name, fit).map(|(edits, score)| (edits, score, row))).collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
    Some(Narrowed { exact, close: ranked.into_iter().map(|(_, _, row)| row).collect() })
}

/// The fuzzy score alone, for the parity table the backend scorer is held to.
pub fn fuzzy_score(name: &str, query: &str) -> Option<i32> {
    let mut matcher = Matcher::new(query)?;
    matcher.load(super::Name::of(name));
    matcher.align().map(|(score, _)| score)
}
