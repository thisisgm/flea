// Settings search: a query gathers rows and hints from every section, a matching heading brings its whole group.
use super::rows::{focusable, Kind, Row};
use crate::i18n::I18n;

// A typed run inside a label, in characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub start: usize,
    pub length: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    // The row's index in its section's own rows, so Enter can open the section there.
    pub row: usize,
    pub run: Option<Run>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub section: usize,
    // The heading row's index, or None for rows a section draws before its first heading.
    pub heading: Option<usize>,
    pub heading_run: Option<Run>,
    pub hits: Vec<Hit>,
}

// Sample input: run_of("Show Thumbnails", "thumb") is 5 characters in, 5 long; case-insensitive, None when absent.
pub fn run_of(label: &str, query: &str) -> Option<Run> {
    let needle: Vec<char> = query.trim().chars().flat_map(char::to_lowercase).collect();
    if needle.is_empty() {
        return None;
    }
    let hay: Vec<char> = label.chars().collect();
    let lower: Vec<Vec<char>> = hay.iter().map(|c| c.to_lowercase().collect()).collect();
    'start: for start in 0..hay.len() {
        let mut at = 0;
        for (offset, folded) in lower[start..].iter().enumerate() {
            for c in folded {
                if needle.get(at) != Some(c) {
                    continue 'start;
                }
                at += 1;
            }
            if at == needle.len() {
                return Some(Run { start, length: offset + 1 });
            }
        }
    }
    None
}

// What a row is searched by: its label, else a hint's own sentence; a ruler or the key examples have none.
fn searched(row: &Row, i18n: &I18n) -> String {
    match row.kind {
        Kind::Ruler | Kind::KeyPreview => String::new(),
        _ => row.label.resolve(i18n),
    }
}

// sections: each section's index with the rows it draws. Results keep section order, then row order.
pub fn search(sections: &[(usize, Vec<Row>)], query: &str, i18n: &I18n) -> Vec<Group> {
    let mut out = Vec::new();
    if query.trim().is_empty() {
        return out;
    }
    for (section, rows) in sections {
        let mut group: Option<Group> = None;
        let mut heading_hit = false;
        let flush = |group: &mut Option<Group>, out: &mut Vec<Group>| {
            if let Some(found) = group.take().filter(|g| g.heading_run.is_some() || !g.hits.is_empty()) {
                out.push(found);
            }
        };
        for (index, row) in rows.iter().enumerate() {
            if row.kind == Kind::Group || group.is_none() {
                flush(&mut group, &mut out);
                let heading = (row.kind == Kind::Group).then_some(index);
                let heading_run = heading.and_then(|_| run_of(&row.label.resolve(i18n), query));
                heading_hit = heading_run.is_some();
                group = Some(Group { section: *section, heading, heading_run, hits: Vec::new() });
                if heading.is_some() {
                    continue;
                }
            }
            let run = run_of(&searched(row, i18n), query);
            if heading_hit || run.is_some() {
                group.as_mut().expect("group is open").hits.push(Hit { row: index, run });
            }
        }
        flush(&mut group, &mut out);
    }
    out
}

// Enter opens the first result's section at its row; a heading with no rows opens at the heading itself.
pub fn first_target(groups: &[Group], sections: &[(usize, Vec<Row>)]) -> Option<(usize, usize)> {
    let group = groups.first()?;
    let rows = &sections.iter().find(|(section, _)| *section == group.section)?.1;
    let row = group.hits.iter().map(|hit| hit.row).find(|row| focusable(&rows[*row]))
        .or_else(|| group.hits.first().map(|hit| hit.row))
        .or(group.heading)?;
    Some((group.section, row))
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
