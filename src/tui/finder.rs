//! Filename traversal keeps the listing intact and owns a separate generation-bound query.
use super::{
    model::Model,
    wire::{count, number, text, word, Wire},
};
use crate::jsondoc::Json;
use std::io;

#[derive(Default)]
pub struct Finder {
    pub query: String,
    id: usize,
    listing: Option<usize>,
    waiting: bool,
    previous: bool,
    rows: Vec<usize>,
}
pub fn query(m: &mut Model, query: String, previous: bool, wire: &mut Wire) -> io::Result<()> {
    let finder = &mut m.parity17.finder;
    if finder.query == query && finder.listing == wire.listing {
        return Ok(());
    }
    if query.is_empty() {
        finder.query.clear();
        finder.rows.clear();
        finder.waiting = false;
        return Ok(());
    }
    finder.query = query;
    finder.previous = previous;
    finder.id = finder.id.wrapping_add(1).max(1);
    finder.listing = wire.listing;
    finder.waiting = true;
    wire.send(vec![
        ("c", word("filter")),
        ("traverse", Json::Bool(true)),
        ("id", number(finder.id)),
        ("query", word(&finder.query)),
    ])
}
pub fn next(m: &mut Model, previous: bool, wire: &mut Wire) -> io::Result<()> {
    let finder = &m.parity17.finder;
    if finder.query.is_empty() {
        return Ok(());
    }
    if finder.listing != wire.listing {
        return query(m, finder.query.clone(), previous, wire);
    }
    if finder.waiting {
        return Ok(());
    }
    let shown = super::filter::answered(m);
    let rows: Vec<_> = finder
        .rows
        .iter()
        .copied()
        .filter(|i| shown.is_none_or(|shown| shown.binary_search(i).is_ok()))
        .collect();
    let row = if previous {
        rows.iter()
            .rev()
            .find(|&&i| i < m.cursor)
            .or_else(|| rows.iter().rev().find(|&&i| i != m.cursor))
    } else {
        rows.iter()
            .find(|&&i| i > m.cursor)
            .or_else(|| rows.iter().find(|&&i| i != m.cursor))
    };
    if let Some(&row) = row {
        m.sort_anchor = None;
        m.cursor = row;
        m.window(wire)?;
    }
    Ok(())
}
// Sample input: {"t":"found","id":1,"query":"a","rows":[0,2],"listing":3}
// A folder's listed line carries no listing stamp, so a find asked in one is judged by navigation alone.
pub fn receive(m: &mut Model, value: &Json, wire: &mut Wire) -> io::Result<()> {
    let finder = &mut m.parity17.finder;
    if !finder.waiting
        || finder.id != count(value, "id")
        || finder.query != text(value, "query")
        || finder.listing != wire.listing
        || (wire.listing.is_some()
            && value.get("listing").map(|_| count(value, "listing")) != wire.listing)
        || m.pending.is_some()
    {
        return Ok(());
    }
    if !text(value, "error").is_empty() {
        finder.waiting = false;
        finder.rows.clear();
        m.fail(text(value, "error").into());
        return Ok(());
    }
    finder.rows = value
        .get("rows")
        .and_then(Json::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(Json::as_f64)
        .map(|i| i as usize)
        .filter(|&i| i < m.total)
        .collect();
    finder.rows.sort_unstable();
    finder.rows.dedup();
    finder.waiting = false;
    let previous = finder.previous;
    if finder.rows.binary_search(&m.cursor).is_ok() {
        return Ok(());
    }
    next(m, previous, wire)
}
