// Decodes bounded shared-journal entries, identities, and replay records.
use super::shape;
use super::{
    checked_op, checked_path, checked_text, get, get_str, parse_i64, parse_u32, parse_u64,
};
use crate::backend::redo::Replay;
use crate::backend::undo::{Entry, ItemIdentity, Step};
use crate::backend::undoshare::{Doc, StoredRedo};
#[cfg(test)]
use crate::backend::undostage;
use crate::backend::{fatnames, filegen, link, mergeapply, redo, tagsundo, trash};
use crate::error::FleaError;
use crate::jsondoc::Json;

const MAX_STEPS: usize = 1_000_000;
const MAX_READ_ENTRIES: usize = 10_000;
// mode() & 0o170000 answers one of these for every file the kernel can stat.
const KNOWN_KINDS: [u32; 7] = [
    0o140000, 0o120000, 0o100000, 0o060000, 0o040000, 0o020000, 0o010000,
];

fn decode_identity(value: &Json) -> Option<ItemIdentity> {
    let pairs = value.as_object()?;
    shape::fields(pairs, &["d", "i", "k", "l", "m", "c", "b"])?;
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
    shape::step(pairs)?;
    match get_str(pairs, "k")? {
        "m" => Some(Step::Moved {
            from: checked_path(get(pairs, "from")?)?,
            to: checked_path(get(pairs, "to")?)?,
            before: decode_identity(get(pairs, "before")?)?,
            after: decode_identity(get(pairs, "after")?)?,
            rename_fat: fatnames::decode_renamed(pairs),
        }),
        "c" => Some(Step::Created {
            path: checked_path(get(pairs, "path")?)?,
        }),
        "l" => {
            let kind = match get_str(pairs, "kind")? {
                "relative" => link::LinkKind::Relative,
                "absolute" => link::LinkKind::Absolute,
                "hard" => link::LinkKind::Hard,
                _ => return None,
            };
            Some(Step::Linked {
                path: checked_path(get(pairs, "path")?)?,
                identity: decode_identity(get(pairs, "id")?)?,
                source: checked_path(get(pairs, "src")?)?,
                kind,
            })
        }
        "cp" => Some(Step::Copied {
            from: checked_path(get(pairs, "from")?)?,
            to: checked_path(get(pairs, "to")?)?,
            source: decode_identity(get(pairs, "src")?)?,
            created: decode_identity(get(pairs, "made")?)?,
            manifest: None,
            manifest_nonce: get(pairs, "nonce").and_then(parse_u64),
            merge: mergeapply::decode_merge(pairs),
            rename_fat: fatnames::decode_renamed(pairs),
        }),
        "md" => Some(Step::MadeDir {
            path: checked_path(get(pairs, "path")?)?,
            identity: decode_identity(get(pairs, "id")?)?,
        }),
        "mf" => Some(Step::MadeFile {
            path: checked_path(get(pairs, "path")?)?,
            identity: decode_identity(get(pairs, "id")?)?,
        }),
        "re" => mergeapply::decode_emptied(pairs),
        "t" => trash::decode_step(pairs),
        "pm" => Some(Step::Mode {
            path: checked_path(get(pairs, "path")?)?,
            before: get(pairs, "before").and_then(parse_u32)?,
            after: get(pairs, "after").and_then(parse_u32)?,
            dev: get(pairs, "dev").and_then(parse_u64)?,
            ino: get(pairs, "ino").and_then(parse_u64)?,
            born: decode_born(get(pairs, "born")?)?,
            generation: filegen::decode_generation(pairs)?,
        }),
        "local" => None,
        "barrier" => Some(Step::Barrier),
        "tg" => tagsundo::decode(pairs),
        _ => None,
    }
}

fn decode_entry(value: &Json) -> Option<Entry> {
    let pairs = value.as_object()?;
    shape::fields(pairs, &["op", "steps"])?;
    let steps = get(pairs, "steps")?.as_array()?;
    if steps.len() > MAX_STEPS {
        return None;
    }
    let mut out = Vec::with_capacity(steps.len().min(1024));
    for step in steps {
        out.push(decode_step(step)?);
    }
    Some(Entry {
        op: checked_op(get(pairs, "op")?)?,
        steps: out,
    })
}

fn decode_replay_step(value: &Json) -> Option<redo::SavedStep> {
    let pairs = value.as_object()?;
    shape::fields(pairs, &["s", "in", "parent"])?;
    let step = decode_step(get(pairs, "s")?)?;
    let input = match get(pairs, "in")? {
        Json::Null => None,
        id => Some(decode_identity(id)?),
    };
    let parent = match get(pairs, "parent")? {
        Json::Null => None,
        Json::Arr(items) if items.len() == 2 => {
            Some((checked_path(&items[0])?, decode_identity(&items[1])?))
        }
        _ => return None,
    };
    Some((step, input, parent))
}

fn decode_redo(value: &Json) -> Option<StoredRedo> {
    let pairs = value.as_object()?;
    if let Some(err) = get(pairs, "err") {
        shape::fields(pairs, &["err"])?;
        let fields = err.as_object()?;
        shape::fields(fields, &["where", "path", "msg"])?;
        return Some(StoredRedo::Err(FleaError {
            where_: checked_text(get(fields, "where")?)?,
            path: match get(fields, "path") {
                None => String::new(),
                Some(v) => v.as_str()?.to_string(),
            },
            msg: checked_text(get(fields, "msg")?)?,
        }));
    }
    shape::fields(pairs, &["op", "steps"])?;
    let steps = get(pairs, "steps")?.as_array()?;
    if steps.len() > MAX_STEPS {
        return None;
    }
    let mut out = Vec::with_capacity(steps.len().min(1024));
    for step in steps {
        out.push(decode_replay_step(step)?);
    }
    Some(StoredRedo::Ok(Replay::from_steps(
        checked_op(get(pairs, "op")?)?,
        out,
    )))
}

#[cfg(test)]
pub(crate) fn decode(text: &str) -> Option<Doc> {
    match undostage::read(text) {
        undostage::Read::Doc(doc) => Some(doc),
        _ => None,
    }
}

pub(crate) fn decode_pairs(pairs: &[(String, Json)]) -> Option<Doc> {
    let push_gen = match get(pairs, "gen") {
        Some(value) => parse_u64(value)?,
        None => 0,
    };
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
