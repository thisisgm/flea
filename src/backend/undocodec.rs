// The shared journal's wire form, both directions; a Copied nonce reattaches only in its recorder.
use super::undoshare::{Doc, StoredRedo};
use crate::backend::redo::Replay;
use crate::backend::undo::{Entry, ItemIdentity, Step};
use crate::error::FleaError;
use crate::jsondoc::{parse, Json};
use std::path::PathBuf;

pub(crate) const VERSION: u32 = 3;
// A single record is checked, never trusted: absolute paths, closed kind sets, parsed numbers.
const MAX_OP: usize = 128;
const MAX_PATH: usize = 4096;
const MAX_TEXT: usize = 8192;
const MAX_STEPS: usize = 1_000_000;
const MAX_READ_ENTRIES: usize = 10_000;
// mode() & 0o170000 answers one of these for every file the kernel can stat.
const KNOWN_KINDS: [u32; 7] = [0o140000, 0o120000, 0o100000, 0o060000, 0o040000, 0o020000, 0o010000];

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

fn get_str<'a>(pairs: &'a [(String, Json)], key: &str) -> Option<&'a str> {
    match get(pairs, key) {
        Some(Json::Str(text)) => Some(text),
        _ => None,
    }
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

fn decode_identity(value: &Json) -> Option<ItemIdentity> {
    let pairs = value.as_object()?;
    let kind = get(pairs, "k").and_then(parse_u32)?;
    if !KNOWN_KINDS.contains(&kind) {
        return None;
    }
    let pair = |key: &str| -> Option<(i64, i64)> {
        let items = get(pairs, key)?.as_array()?;
        if items.len() != 2 {
            return None;
        }
        Some((parse_i64(&items[0])?, parse_i64(&items[1])?))
    };
    Some(ItemIdentity::from_parts(
        get(pairs, "d").and_then(parse_u64)?,
        get(pairs, "i").and_then(parse_u64)?,
        kind,
        get(pairs, "l").and_then(parse_u64)?,
        pair("m")?,
        pair("c")?,
        // A record from before birth time was kept has no "b", and decodes as unknown.
        match get(pairs, "b") {
            None => None,
            Some(value) => decode_born(value)?,
        },
    ))
}

// Sample input: null or [1790975225,498364116]; null is a birth time the writer never knew.
pub(crate) fn decode_born(value: &Json) -> Option<Option<(u64, u32)>> {
    match value {
        Json::Null => Some(None),
        v => {
            let items = v.as_array()?;
            if items.len() != 2 {
                return None;
            }
            Some(Some((parse_u64(&items[0])?, parse_u32(&items[1])?)))
        }
    }
}

fn decode_step(value: &Json) -> Option<Step> {
    let pairs = value.as_object()?;
    match get_str(pairs, "k")? {
        "m" => Some(Step::Moved {
            from: checked_path(get(pairs, "from")?)?,
            to: checked_path(get(pairs, "to")?)?,
            before: decode_identity(get(pairs, "before")?)?,
            after: decode_identity(get(pairs, "after")?)?,
            rename_fat: super::fatnames::decode_renamed(pairs),
        }),
        "c" => Some(Step::Created { path: checked_path(get(pairs, "path")?)? }),
        "l" => {
            let kind = match get_str(pairs, "kind")? {
                "relative" => super::link::LinkKind::Relative,
                "absolute" => super::link::LinkKind::Absolute,
                "hard" => super::link::LinkKind::Hard,
                _ => return None,
            };
            Some(Step::Linked {
                path: checked_path(get(pairs, "path")?)?,
                identity: decode_identity(get(pairs, "id")?)?,
                source: checked_path(get(pairs, "src")?)?,
                kind,
            })
        }
        "cp" => {
            Some(Step::Copied {
                from: checked_path(get(pairs, "from")?)?,
                to: checked_path(get(pairs, "to")?)?,
                source: decode_identity(get(pairs, "src")?)?,
                created: decode_identity(get(pairs, "made")?)?,
                manifest: None,
                manifest_nonce: get(pairs, "nonce").and_then(parse_u64),
                merge: super::mergeapply::decode_merge(pairs),
                rename_fat: super::fatnames::decode_renamed(pairs),
            })
        }
        "md" => Some(Step::MadeDir {
            path: checked_path(get(pairs, "path")?)?,
            identity: decode_identity(get(pairs, "id")?)?,
        }),
        "mf" => Some(Step::MadeFile {
            path: checked_path(get(pairs, "path")?)?,
            identity: decode_identity(get(pairs, "id")?)?,
        }),
        "re" => super::mergeapply::decode_emptied(pairs),
        "t" => super::trash::decode_step(pairs),
        "pm" => Some(Step::Mode {
            path: checked_path(get(pairs, "path")?)?,
            before: get(pairs, "before").and_then(parse_u32)?,
            after: get(pairs, "after").and_then(parse_u32)?,
            dev: get(pairs, "dev").and_then(parse_u64)?,
            ino: get(pairs, "ino").and_then(parse_u64)?,
            born: decode_born(get(pairs, "born")?)?,
            generation: super::filegen::decode_generation(pairs)?,
        }),
        "local" => None,
        "barrier" => Some(Step::Barrier),
        "tg" => super::tagsundo::decode(pairs),
        _ => None,
    }
}

fn decode_entry(value: &Json) -> Option<Entry> {
    let pairs = value.as_object()?;
    let steps = get(pairs, "steps")?.as_array()?;
    if steps.len() > MAX_STEPS {
        return None;
    }
    let mut out = Vec::with_capacity(steps.len().min(1024));
    for step in steps {
        out.push(decode_step(step)?);
    }
    Some(Entry { op: checked_op(get(pairs, "op")?)?, steps: out })
}

fn decode_replay_step(value: &Json) -> Option<super::redo::SavedStep> {
    let pairs = value.as_object()?;
    let step = decode_step(get(pairs, "s")?)?;
    let input = match get(pairs, "in")? {
        Json::Null => None,
        id => Some(decode_identity(id)?),
    };
    let parent = match get(pairs, "parent")? {
        Json::Null => None,
        Json::Arr(items) if items.len() == 2 => Some((checked_path(&items[0])?, decode_identity(&items[1])?)),
        _ => return None,
    };
    Some((step, input, parent))
}

fn decode_redo(value: &Json) -> Option<StoredRedo> {
    let pairs = value.as_object()?;
    if let Some(err) = get(pairs, "err") {
        let fields = err.as_object()?;
        return Some(StoredRedo::Err(FleaError {
            where_: checked_text(get(fields, "where")?)?,
            path: get_str(fields, "path").unwrap_or("").to_string(),
            msg: checked_text(get(fields, "msg")?)?,
        }));
    }
    let steps = get(pairs, "steps")?.as_array()?;
    if steps.len() > MAX_STEPS {
        return None;
    }
    let mut out = Vec::with_capacity(steps.len().min(1024));
    for step in steps {
        out.push(decode_replay_step(step)?);
    }
    Some(StoredRedo::Ok(Replay::from_steps(checked_op(get(pairs, "op")?)?, out)))
}

#[cfg(test)]
pub(crate) fn decode(text: &str) -> Option<Doc> {
    match super::undostage::read(text) {
        super::undostage::Read::Doc(doc) => Some(doc),
        _ => None,
    }
}

pub(crate) fn decode_pairs(pairs: &[(String, Json)]) -> Option<Doc> {
    let push_gen = get(pairs, "gen").and_then(parse_u64).unwrap_or(0);
    let job_gen = match get(pairs, "jobGen") {
        Some(value) => parse_u64(value)?,
        None => 0,
    };
    let undo_items = get(pairs, "undo")?.as_array()?;
    let redo_items = get(pairs, "redo")?.as_array()?;
    if undo_items.len() > MAX_READ_ENTRIES || redo_items.len() > MAX_READ_ENTRIES {
        return None;
    }
    let mut undo = Vec::with_capacity(undo_items.len());
    for item in undo_items {
        undo.push(decode_entry(item)?);
    }
    let mut redo = Vec::with_capacity(redo_items.len());
    for item in redo_items {
        redo.push(decode_redo(item)?);
    }
    Some(Doc {
        undo,
        redo,
        push_gen,
        job_gen,
    })
}

// A newer writer owns the file; the reader falls back to memory and never rewrites it.
pub(crate) fn is_newer_version(text: &str) -> bool {
    #[cfg(test)]
    super::undoprobe::decode();
    let Ok(root) = parse(text) else { return false };
    let Some(pairs) = root.as_object() else { return false };
    match get(pairs, "v") {
        Some(Json::Num(literal)) => literal.parse::<u64>().is_ok_and(|v| v > VERSION as u64),
        _ => false,
    }
}
