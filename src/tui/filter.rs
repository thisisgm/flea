use super::{model::Model, wire::{count, number, text, word, Wire}};
use crate::jsondoc::Json;
use std::io;

// The backend's filter request matches every name of the listing, and only the drawn matches are fetched as rows.
#[derive(Default)]
pub struct Matches {
    // The latest accepted answer, original listing indices in ascending order.
    rows: Option<Vec<usize>>,
    // The query and id of the latest request, and an answer naming any other is dropped.
    asked: String,
    id: usize,
    // True from a request until its answer, the way a pending listing holds actions.
    pub waiting: bool,
    // The position in rows of the first drawn match.
    pub top: usize,
    // A committed line's Enter that acts on the highlighted row once the answer lands (enter_once.rs).
    pub enter_after: bool,
}
impl Matches {
    // A renumbered listing or a cleared query leaves no answer standing.
    pub fn reset(&mut self) {
        self.rows = None;
        self.asked.clear();
        self.waiting = false;
        self.top = 0;
        self.enter_after = false;
    }
}
// Until the answer to the current query arrives, the query narrows the held rows, the same substring the backend matches.
fn loaded(m: &Model) -> Vec<usize> {
    let folded = m.filter.to_lowercase();
    m.rows.iter().filter(|(_, row)| row.name.to_lowercase().contains(&folded)).map(|(&i, _)| i).collect()
}
// The answer to the current query, once it has arrived; an older answer stays out of the drawn rows and the count.
pub fn answered(m: &Model) -> Option<&[usize]> {
    m.name_filter.rows.as_deref().filter(|_| !m.filter.is_empty() && !m.name_filter.waiting)
}
pub fn shown(m: &Model) -> Vec<usize> {
    answered(m).map_or_else(|| loaded(m), <[usize]>::to_vec)
}
pub fn visible(m: &Model) -> Vec<usize> {
    match answered(m) {
        Some(rows) => rows[m.name_filter.top.min(rows.len())..].iter().take(m.file_capacity()).copied().collect(),
        None => loaded(m).into_iter().take(m.file_capacity()).collect(),
    }
}
pub fn count_shown(m: &Model) -> usize {
    answered(m).map_or_else(|| loaded(m).len(), <[usize]>::len)
}
// The scroll lane's position and length, over the drawn matches under an answered filter and over the listing otherwise.
pub fn lane(m: &Model) -> (usize, usize) {
    match answered(m) {
        Some(rows) => (m.name_filter.top, rows.len()),
        None if !m.filter.is_empty() => (0, count_shown(m)),
        None => (m.top, m.total),
    }
}
pub fn change(m: &mut Model, query: String, wire: &mut Wire) -> io::Result<()> {
    if query.is_empty() { m.name_filter.reset(); }
    m.filter = query;
    m.window(wire)
}
pub fn window(m: &mut Model, wire: &mut Wire) -> io::Result<()> {
    if m.name_filter.asked != m.filter {
        m.name_filter.asked = m.filter.clone();
        m.name_filter.id = m.name_filter.id.wrapping_add(1);
        m.name_filter.waiting = true;
        return wire.send(vec![("c", word("filter")), ("query", word(&m.filter)), ("id", number(m.name_filter.id))]);
    }
    let Some(rows) = answered(m) else { return Ok(()); };
    if let Ok(position) = rows.binary_search(&m.cursor) {
        let old_top = m.name_filter.top;
        if position < old_top { m.name_filter.top = position; }
        if position >= m.name_filter.top + m.file_capacity() { m.name_filter.top = position + 1 - m.file_capacity(); }
        if m.name_filter.top != old_top { m.scrolled_at = Some(std::time::Instant::now()); }
    }
    let missing: Vec<usize> = visible(m).into_iter().filter(|i| !m.rows.contains_key(i)).collect();
    for run in missing.chunk_by(|a, b| *b == a + 1) {
        wire.send(vec![("c", word("window")), ("start", number(run[0])), ("count", number(run.len()))])?;
    }
    Ok(())
}
// Sample input: {"t":"filtered","id":7,"query":".PNG","n":1300,"rows":[0,2,4],"ms":0.012,"listing":1}.
pub fn receive(m: &mut Model, value: &Json, wire: &mut Wire) -> io::Result<()> {
    if !m.name_filter.waiting || count(value, "id") != m.name_filter.id || text(value, "query") != m.filter { return Ok(()); }
    let rows: Vec<usize> = value.get("rows").and_then(Json::as_array).unwrap_or(&[]).iter()
        .filter_map(Json::as_f64).map(|i| i as usize).filter(|&i| i < m.total).collect();
    m.selected.retain(|i| rows.binary_search(i).is_ok());
    if rows.binary_search(&m.cursor).is_err() { m.cursor = rows.first().copied().unwrap_or(m.top); }
    // A hidden row leaves the held rows, and actions.rs refuses a row action at a cursor it does not hold, so no action reaches a row the filter does not draw.
    m.rows.retain(|i, _| rows.binary_search(i).is_ok());
    m.name_filter.rows = Some(rows);
    m.name_filter.waiting = false;
    m.name_filter.top = 0;
    m.remember_selection();
    window(m, wire)
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
