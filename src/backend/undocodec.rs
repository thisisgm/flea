// The shared journal's wire form, both directions; a Copied nonce reattaches only in its recorder.
use super::undoshare::StoredRedo;
#[cfg(test)]
use super::undoshare::Doc;
use crate::backend::undo::{Entry, ItemIdentity, Step};
use crate::jsondoc::Json;
use std::path::PathBuf;

#[path = "undodecode.rs"]
mod read;
#[cfg(test)]
pub(crate) use read::decode;
pub(crate) use read::{decode_born, decode_pairs};

#[path = "undoshape.rs"]
mod shape;
use shape::get_str;

pub(crate) const VERSION: u32 = 3;
// A single record is checked, never trusted: absolute paths, closed kind sets, parsed numbers.
const MAX_OP: usize = 128;
const MAX_PATH: usize = 4096;
const MAX_TEXT: usize = 8192;

pub(crate) fn s(value: &str) -> Json {
    Json::Str(value.to_string())
}

pub(crate) fn n(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}

pub(crate) fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

pub(crate) fn arr(items: Vec<Json>) -> Json {
    Json::Arr(items)
}

fn identity(id: &ItemIdentity) -> Json {
    let (dev, ino, kind, len, mtime, changed, born) = id.to_parts();
    obj(vec![
        ("d", n(dev)),
        ("i", n(ino)),
        ("k", n(kind)),
        ("l", n(len)),
        ("m", arr(vec![n(mtime.0), n(mtime.1)])),
        ("c", arr(vec![n(changed.0), n(changed.1)])),
        ("b", born_json(born)),
    ])
}

// Null stands for a birth time the filesystem does not keep.
pub(crate) fn born_json(born: Option<(u64, u32)>) -> Json {
    match born {
        Some((sec, nsec)) => arr(vec![n(sec), n(nsec)]),
        None => Json::Null,
    }
}

fn link_kind(kind: &super::link::LinkKind) -> &'static str {
    match kind {
        super::link::LinkKind::Relative => "relative",
        super::link::LinkKind::Absolute => "absolute",
        super::link::LinkKind::Hard => "hard",
    }
}

fn step(step: &Step) -> Json {
    match step {
        Step::Moved { from, to, before, after, rename_fat } => obj(super::fatnames::mark_renamed(vec![
            ("k", s("m")), ("from", s(&from.to_string_lossy())), ("to", s(&to.to_string_lossy())),
            ("before", identity(before)), ("after", identity(after)),
        ], *rename_fat)),
        Step::Created { path } => obj(vec![("k", s("c")), ("path", s(&path.to_string_lossy()))]),
        Step::Linked { path, identity: id, source, kind } => obj(vec![
            ("k", s("l")), ("path", s(&path.to_string_lossy())), ("id", identity(id)),
            ("src", s(&source.to_string_lossy())), ("kind", s(link_kind(kind))),
        ]),
        // Sample input: {"k":"cp","from":"/a","to":"/b","src":{...},"made":{...},"nonce":7}.
        Step::Copied { from, to, source, created, manifest_nonce, merge, rename_fat, .. } => {
            let mut pairs = vec![
                ("k", s("cp")), ("from", s(&from.to_string_lossy())), ("to", s(&to.to_string_lossy())),
                ("src", identity(source)), ("made", identity(created)),
            ];
            if let Some(nonce) = manifest_nonce {
                pairs.push(("nonce", n(nonce)));
            }
            obj(super::fatnames::mark_renamed(super::mergeapply::mark_merge(pairs, *merge), *rename_fat))
        }
        Step::MadeDir { path, identity: id } => obj(vec![
            ("k", s("md")), ("path", s(&path.to_string_lossy())), ("id", identity(id)),
        ]),
        Step::MadeFile { path, identity: id } => obj(vec![
            ("k", s("mf")), ("path", s(&path.to_string_lossy())), ("id", identity(id)),
        ]),
        Step::RemovedEmptyDir { path, mode } => super::mergeapply::encode_emptied(path, *mode),
        Step::Trashed(entry) => super::trash::encode_step(entry),
        Step::Mode { path, before, after, dev, ino, born, generation } => obj(super::filegen::with_generation(vec![
            ("k", s("pm")), ("path", s(&path.to_string_lossy())), ("before", n(before)), ("after", n(after)),
            ("dev", n(dev)), ("ino", n(ino)), ("born", born_json(*born)),
        ], *generation)),
        // Sample input: {"k":"barrier"}.
        Step::Barrier => obj(vec![("k", s("barrier"))]),
        Step::Tags { path, before, after, dev, ino, born, generation } => super::tagsundo::encode(path, before, after, *dev, *ino, (*born, *generation)),
    }
}

pub(crate) fn entry(entry: &Entry) -> Json {
    obj(vec![
        ("op", s(&entry.op)),
        ("steps", arr(entry.steps.iter().map(step).collect())),
    ])
}

fn replay_step(st: &Step, input: &Option<ItemIdentity>, parent: &Option<(PathBuf, ItemIdentity)>) -> Json {
    let mut pairs = vec![("s", step(st))];
    match input {
        Some(id) => pairs.push(("in", identity(id))),
        None => pairs.push(("in", Json::Null)),
    }
    match parent {
        Some((path, id)) => pairs.push(("parent", arr(vec![s(&path.to_string_lossy()), identity(id)]))),
        None => pairs.push(("parent", Json::Null)),
    }
    obj(pairs)
}

pub(crate) fn stored_redo(stored: &StoredRedo) -> Json {
    match stored {
        StoredRedo::Ok(replay) => {
            let (op, steps) = replay.steps_data();
            obj(vec![
                ("op", s(&op)),
                ("steps", arr(steps.iter().map(|(st, input, parent)| replay_step(st, input, parent)).collect())),
            ])
        }
        StoredRedo::Err(error) => obj(vec![
            ("err", obj(vec![("where", s(&error.where_)), ("path", s(&error.path)), ("msg", s(&error.msg))])),
        ]),
    }
}

// The whole document as a tree; the byte pin compares the piecewise render against this form.
#[cfg(test)]
pub(crate) fn encode(doc: &Doc) -> Json {
    let mut fields = vec![("v", n(VERSION)), ("gen", n(doc.push_gen))];
    if doc.job_gen != 0 {
        fields.push(("jobGen", n(doc.job_gen)));
    }
    fields.extend([
        ("undo", arr(doc.undo.iter().map(entry).collect())),
        ("redo", arr(doc.redo.iter().map(stored_redo).collect())),
    ]);
    obj(fields)
}

pub(crate) fn get<'a>(pairs: &'a [(String, Json)], key: &str) -> Option<&'a Json> {
    pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

pub(crate) fn parse_u64(value: &Json) -> Option<u64> {
    match value {
        Json::Num(literal) => literal.parse().ok(),
        _ => None,
    }
}

fn parse_i64(value: &Json) -> Option<i64> {
    match value {
        Json::Num(literal) => literal.parse().ok(),
        _ => None,
    }
}

pub(crate) fn parse_u32(value: &Json) -> Option<u32> {
    match value {
        Json::Num(literal) => literal.parse().ok(),
        _ => None,
    }
}

// Absolute paths only: a relative one names the reader's own directory, so it is foreign.
pub(crate) fn checked_path(value: &Json) -> Option<PathBuf> {
    match value {
        Json::Str(text) if !text.is_empty() && text.len() <= MAX_PATH
            && text.starts_with('/') && !text.contains('\0') => Some(PathBuf::from(text)),
        _ => None,
    }
}

pub(crate) fn checked_text(value: &Json) -> Option<String> {
    match value {
        Json::Str(text) if text.len() <= MAX_TEXT && !text.contains('\0') => Some(text.clone()),
        _ => None,
    }
}

// An empty uri is a record when gio listed nothing; bounded and NUL-free is the whole check.
pub(crate) fn checked_uri(value: &Json) -> Option<String> {
    checked_text(value)
}

fn checked_op(value: &Json) -> Option<String> {
    match value {
        Json::Str(text) if !text.is_empty() && text.len() <= MAX_OP && !text.contains('\0') => Some(text.clone()),
        _ => None,
    }
}
