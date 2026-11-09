use super::{
    model::{Model, Row},
    wire::{count, text, Wire},
};
use crate::jsondoc::{self, Json};
use flea::ui_logic_file_search::{parse_line, ContentsMatch, Line, Request, Scope, Session};
use std::{collections::BTreeMap, io, path::PathBuf};

mod failure;

#[derive(Clone)]
pub(super) struct Query {
    pub gen: usize,
    pub root: PathBuf,
    pub query: String,
    pub contents: bool,
    pub scope: Scope,
}
#[derive(Default)]
struct Block {
    listing: usize,
    listed: Option<Json>,
    matches: BTreeMap<usize, ContentsMatch>,
}
#[derive(Default)]
pub(super) struct State {
    pub mode: bool,
    pub session: flea::ui_logic_file_search::Session,
    next: usize,
    pub waiting: bool,
    pub cancelling: bool,
    asked: Option<Query>,
    pub active: Option<Query>,
    pub listing: usize,
    pub matches: BTreeMap<usize, ContentsMatch>,
    block: Block,
    previous: Option<(String, String, bool, Option<PathBuf>)>,
}

pub(super) fn row_height(m: &Model) -> usize {
    if m.prompts.trash.is_none() && m.find.active.as_ref().is_some_and(|q| q.contents) {
        2
    } else {
        1
    }
}
pub(super) fn holding(m: &Model) -> bool {
    m.find.waiting || m.find.cancelling
}
pub(super) fn operable(m: &Model, index: usize) -> bool {
    !holding(m) && displayed_operable(m, index)
}
pub(super) fn displayed_operable(m: &Model, index: usize) -> bool {
    !m.find.active.as_ref().is_some_and(|q| q.contents)
        || m.find.matches.get(&index).is_some_and(|hit| hit.operable)
}
pub(super) fn guarded(m: &Model) -> bool {
    m.find.waiting
        || m.find.cancelling
        || !operable(m, m.cursor)
        || m.selected.iter().any(|index| !operable(m, *index))
}
pub(super) fn send(wire: &mut Wire, request: Request) -> io::Result<()> {
    match jsondoc::parse(&request.line()).map_err(io::Error::other)? {
        Json::Obj(fields) => wire.send(
            fields
                .iter()
                .map(|(key, value)| (key.as_str(), value.clone()))
                .collect(),
        ),
        _ => unreachable!(),
    }
}
pub(super) fn start(
    m: &mut Model,
    query: String,
    folder: PathBuf,
    wire: &mut Wire,
) -> io::Result<bool> {
    let home = std::env::var("HOME").unwrap_or_default();
    let Some(root) = Session::scope_root(&folder.to_string_lossy(), &home, m.find.session.scope)
    else {
        m.find.session.scope = m.find.active.as_ref().map_or(Scope::Folder, |q| q.scope);
        return Ok(false);
    };
    let root = PathBuf::from(root);
    if m.find.previous.is_none() {
        m.find.previous = Some((
            m.search.clone(),
            m.search_query.clone(),
            m.searching,
            m.search_from.clone(),
        ));
    }
    m.search_from.get_or_insert_with(|| folder.clone());
    m.find.next = m
        .find
        .next
        .checked_add(1)
        .expect("search generation exhausted");
    let ask = Query {
        gen: m.find.next,
        root,
        query,
        contents: m.find.mode,
        scope: m.find.session.scope,
    };
    m.find.waiting = true;
    m.find.cancelling = false;
    m.find.block = Block::default();
    m.find.asked = Some(ask.clone());
    m.searching = true;
    m.search = m.prompts.i18n.tr("find.starting");
    m.sort_anchor = None;
    let count = m.search_count(ask.contents);
    let result = super::actions::send_find(m, &ask, &folder, count, wire);
    if result.is_err() {
        restore(m);
    }
    result.map(|()| true)
}
pub(super) fn cancel(m: &mut Model, wire: &mut Wire) -> io::Result<()> {
    m.find.cancelling = true;
    m.find.block = Block::default();
    send(wire, Request::Cancel)
}
pub(super) fn navigate(m: &mut Model, wire: &mut Wire) -> io::Result<()> {
    if m.find.cancelling && m.find.asked.is_some() {
        send(wire, Request::Close)?;
    }
    m.find.asked = None;
    m.find.waiting = false;
    m.find.cancelling = false;
    m.find.block = Block::default();
    m.find.previous = None;
    Ok(())
}
fn restore(m: &mut Model) {
    if let Some((status, query, searching, from)) = m.find.previous.take() {
        m.search = status;
        m.search_query = query;
        m.searching = searching;
        m.search_from = from;
    } else {
        m.searching = false;
        m.search = super::prompts::found(m, 0);
    }
    if let Some(active) = &m.find.active {
        m.find.session.scope = active.scope;
    }
    m.find.asked = m.find.active.clone();
    m.find.waiting = false;
    m.find.cancelling = false;
    m.find.block = Block::default();
}
fn bad(m: &mut Model) {
    restore(m);
    m.fail(m.prompts.i18n.tr("tui.find.invalid_response"));
}
fn block(m: &mut Model, listing: usize) {
    if m.find.block.listing != listing {
        m.find.block = Block {
            listing,
            ..Default::default()
        };
    }
}

// A tagged search page is adopted with its matching metadata, never through the untagged listing path.
pub(super) fn receive(m: &mut Model, value: &Json, wire: &mut Wire) -> io::Result<bool> {
    let kind = text(value, "t");
    let gen = count(value, "gen");
    let listing = count(value, "listing");
    if kind == "error" {
        return Ok(failure::receive(m, value));
    }
    if !matches!(
        kind,
        "contentsmatches"
            | "listed"
            | "rows"
            | "searched"
            | "searching"
            | "searchopened"
            | "searchclosed"
    ) {
        return Ok(false);
    }
    if kind == "searchopened" || kind == "searchclosed" {
        return Ok(true);
    }
    if m.pending.is_some() && gen == 0 {
        if kind == "listed" {
            m.find = State {
                next: m.find.next,
                session: std::mem::take(&mut m.find.session),
                ..Default::default()
            };
            return Ok(false);
        }
        if kind == "rows" {
            return Ok(true);
        }
    }
    let Some(ask) = m.find.asked.clone() else {
        return Ok(gen > 0 || kind == "contentsmatches");
    };
    if !ask.contents && !m.find.waiting && gen == 0 && kind == "listed" {
        m.find.asked = None;
        m.find.active = None;
        return Ok(false);
    }
    if !ask.contents && matches!(kind, "listed" | "rows") {
        if m.find.cancelling
            || m.pending.is_some()
            || gen != ask.gen && !(kind == "rows" && gen == 0 && !m.find.waiting)
        {
            return Ok(true);
        }
        if kind == "listed" {
            m.search_path(ask.root.clone(), wire)?;
            m.search_query = ask.query.clone();
            m.find.active = Some(ask);
            m.find.matches.clear();
            m.find.listing = 0;
            m.find.waiting = false;
        }
        return Ok(false);
    }
    if gen != ask.gen || m.pending.is_some() {
        return Ok(true);
    }
    if kind == "searched" {
        let parsed = parse_line(&jsondoc::render(value));
        let Ok(Some(Line::Searched {
            cancelled,
            walking,
            scanned,
            ..
        })) = parsed
        else {
            bad(m);
            return Ok(true);
        };
        if m.find.cancelling && !cancelled {
            m.searching = false;
            m.open(
                m.search_from.clone().unwrap_or_else(|| m.path.clone()),
                wire,
            )?;
        } else if cancelled {
            restore(m);
        } else if !m.find.cancelling && !m.find.waiting {
            m.searching = walking;
            m.search = super::prompts::found(m, scanned);
            m.find.previous = None;
            if !walking {
                m.search_settled(wire)?;
            }
        }
        return Ok(true);
    }
    if m.find.cancelling {
        return Ok(true);
    }
    if kind == "searching" {
        return Ok(true);
    }
    if listing < m.find.listing || listing < m.find.block.listing {
        return Ok(true);
    }
    if listing == 0 {
        bad(m);
        return Ok(true);
    }
    if kind == "contentsmatches" {
        if !ask.contents {
            return Ok(true);
        }
        match parse_line(&jsondoc::render(value)) {
            Ok(Some(Line::ContentsMatches(hits))) => {
                if hits.matches.len() > 4096
                    || m.find.block.matches.len() + hits.matches.len() > 4096
                {
                    bad(m);
                    return Ok(true);
                }
                block(m, listing);
                m.find
                    .block
                    .matches
                    .extend(hits.matches.into_iter().map(|hit| (hit.row, hit)));
            }
            _ => bad(m),
        }
    } else if kind == "listed" {
        if text(value, "path") != ask.root.to_string_lossy() {
            return Ok(true);
        }
        block(m, listing);
        m.find.block.listed = Some(value.clone());
    } else {
        let replacing = m.find.block.listed.is_some();
        if m.find.block.listing != listing
            || !replacing && (m.find.waiting || listing != m.find.listing)
        {
            return Ok(true);
        }
        let Some(values) = value.get("rows").and_then(Json::as_array) else {
            bad(m);
            return Ok(true);
        };
        let start = count(value, "start");
        if !replacing && m.filter.is_empty() && start != m.top {
            m.find.block.matches.clear();
            return Ok(true);
        }
        if values.len() > 4096 || start.checked_add(values.len()).is_none() {
            bad(m);
            return Ok(true);
        }
        let kinds = value.get("kinds").and_then(Json::as_array).unwrap_or(&[]);
        let rows: BTreeMap<_, _> = values
            .iter()
            .enumerate()
            .map(|(i, row)| (start + i, Row::parse(row, kinds)))
            .collect();
        if ask.contents
            && rows.iter().any(|(i, row)| {
                !m.find
                    .block
                    .matches
                    .get(i)
                    .is_some_and(|hit| hit.path == row.name)
            })
        {
            bad(m);
            return Ok(true);
        }
        let total = m
            .find
            .block
            .listed
            .as_ref()
            .map_or(m.total, |listed| count(listed, "n"));
        if start + values.len() > total {
            bad(m);
            return Ok(true);
        }
        if replacing {
            let listed = m.find.block.listed.take().unwrap();
            m.invalidate_rows();
            m.search_path(ask.root.clone(), wire)?;
            m.total = count(&listed, "n");
            m.cursor = m
                .sort_anchor
                .as_ref()
                .filter(|p| p.to_string_lossy() == text(&listed, "anchor"))
                .and_then(|_| listed.get("anchorIndex").and_then(Json::as_f64))
                .filter(|index| *index >= 0.0 && *index < m.total as f64)
                .map_or(0, |index| index as usize);
            m.top = 0;
            m.filter.clear();
            m.search_query = ask.query.clone();
            m.find.active = Some(ask.clone());
            m.find.listing = listing;
            wire.listing = Some(listing);
        }
        let visible = m.visible();
        m.rows
            .retain(|i, _| !replacing && !m.filter.is_empty() && visible.contains(i));
        m.rows.extend(rows);
        m.find
            .matches
            .retain(|i, _| !replacing && (m.rows.contains_key(i) || m.selected.contains(i)));
        m.find
            .matches
            .extend(std::mem::take(&mut m.find.block.matches));
        m.find
            .matches
            .retain(|i, _| m.rows.contains_key(i) || m.selected.contains(i));
        m.find.waiting = false;
        m.search = super::prompts::found(m, 0);
        m.remember_selection();
        m.detail.invalidate();
        if m.cursor >= m.total {
            m.cursor = m.total.saturating_sub(1);
        }
        if !m.rows.contains_key(&m.cursor) && m.total > 0 {
            m.window(wire)?;
        }
    }
    Ok(true)
}
