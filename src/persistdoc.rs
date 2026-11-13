// Persisted documents are read as defaults in memory only when their format is not understood.
use crate::jsondoc::{self, Json};
use std::path::Path;

pub(crate) type Shape = fn(&Json) -> bool;

pub(crate) fn empty() -> Json {
    Json::Obj(Vec::new())
}

pub(crate) fn parse(text: &str, shape: Shape) -> Result<Json, String> {
    let doc = jsondoc::parse(text).map_err(|_| "unparsable document")?;
    if !shape(&doc) {
        return Err("foreign or newer-schema document".to_string());
    }
    Ok(doc)
}

// What a refused shelf document read or write leaves behind, named in every refusal.
const LEFT: &str = "the shelf document was left unchanged";

// Missing is the only disk state a writer may interpret as an empty document.
pub(crate) fn read(path: &Path, shape: Shape) -> Result<Json, String> {
    read_text(path, shape, |_| false)
}

// The pile file alone: 0.3.12 read an empty or blank shelf.json as an empty pile.
pub(crate) fn read_pile(path: &Path) -> Result<Json, String> {
    read_text(path, shelf, |text| text.trim().is_empty())
}

fn read_text(path: &Path, shape: Shape, blank: fn(&str) -> bool) -> Result<Json, String> {
    match crate::uistore::read_write_target(path, LEFT)? {
        None => Ok(empty()),
        Some(text) if blank(&text) => Ok(empty()),
        Some(text) => parse(&text, shape)
            .map_err(|why| format!("{}: {}; file was left unchanged", path.display(), why)),
    }
}

// An unsynced write cut short by a crash leaves an empty file or a piece of the writer's own header, which 0.3.12 rewrote as a miss.
pub(crate) fn torn(bytes: &[u8], own: &[u8]) -> bool {
    bytes.is_empty() || (!own.is_empty() && (own.starts_with(bytes) || bytes.starts_with(own)))
}

// Cache misses do not grant ownership of an existing file with an unknown format; damage under the writer's own header does.
pub(crate) fn replaceable(
    path: &Path,
    limit: u64,
    own: &[u8],
    accepts: impl FnOnce(&[u8]) -> bool,
) -> bool {
    use std::io::Read;
    match std::fs::symlink_metadata(path) {
        Err(e) => return e.kind() == std::io::ErrorKind::NotFound,
        Ok(meta) if !meta.is_file() || meta.len() > limit => return false,
        _ => {}
    }
    let Some(file) = crate::backend::regfile::open_regular(path) else {
        return false;
    };
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).is_ok()
        && bytes.len() as u64 <= limit
        && (torn(&bytes, own) || accepts(&bytes))
}

pub(crate) fn fields(doc: &Json, required: &[&str], optional: &[&str]) -> bool {
    doc.as_object().is_some_and(|pairs| {
        required.iter().all(|key| doc.get(key).is_some())
            && pairs.iter().all(|(key, _)| {
                required.contains(&key.as_str()) || optional.contains(&key.as_str())
            })
    })
}

fn array(doc: &Json, name: &str, item: Shape) -> bool {
    doc.get(name)
        .and_then(Json::as_array)
        .is_some_and(|items| items.iter().all(item))
}

fn text(doc: &Json, name: &str) -> bool {
    doc.get(name).and_then(Json::as_str).is_some()
}

pub(crate) fn count(doc: &Json, name: &str) -> bool {
    doc.get(name)
        .and_then(Json::as_f64)
        .is_some_and(|n| n.is_finite() && n >= 0.0 && n.fract() == 0.0)
}

fn item(doc: &Json) -> bool {
    text(doc, "path")
        && doc.as_object().is_some()
        && doc.get("folder").is_none_or(|v| v.as_bool().is_some())
        && doc.get("pinned").is_none_or(|v| v.as_bool().is_some())
}

pub(crate) fn shelf(doc: &Json) -> bool {
    fields(doc, &["items"], &[]) && array(doc, "items", item)
}

pub(crate) fn piles(doc: &Json) -> bool {
    fields(doc, &["piles"], &[])
        && array(doc, "piles", |pile| {
            fields(pile, &["at", "items"], &[]) && count(pile, "at") && array(pile, "items", item)
        })
}

pub(crate) fn summon(doc: &Json) -> bool {
    fields(doc, &["summon"], &[]) && count(doc, "summon")
}

pub(crate) fn drags(doc: &Json) -> bool {
    fields(doc, &["drags"], &[])
        && array(doc, "drags", |drag| {
            fields(drag, &["token", "intent", "created", "entries"], &[])
                && text(drag, "token")
                && matches!(
                    drag.get("intent").and_then(Json::as_str),
                    Some("move" | "copy")
                )
                && count(drag, "created")
                && array(drag, "entries", |entry| {
                    fields(entry, &["path", "dev", "ino", "bytes"], &[])
                        && text(entry, "path")
                        && count(entry, "dev")
                        && count(entry, "ino")
                        && count(entry, "bytes")
                })
        })
}

pub(crate) fn dests(doc: &Json) -> bool {
    fields(doc, &["dests"], &[]) && array(doc, "dests", |v| v.as_str().is_some())
}

// The legacy picker uses -1 for an unfiltered chooser and for the All files option.
pub(crate) fn reply(doc: &Json) -> bool {
    fields(doc, &["response"], &["uris", "filter"])
        && count(doc, "response")
        && doc
            .get("response")
            .and_then(Json::as_f64)
            .is_some_and(|n| n <= 2.0)
        && doc.get("uris").is_none_or(|v| {
            v.as_array()
                .is_some_and(|uris| uris.iter().all(|v| v.as_str().is_some()))
        })
        && doc.get("filter").is_none_or(|filter| {
            filter
                .as_f64()
                .is_some_and(|n| n.is_finite() && n >= -1.0 && n.fract() == 0.0)
        })
}

pub(crate) fn ui(doc: &Json) -> bool {
    let Some(pairs) = doc.as_object() else {
        return false;
    };
    if doc.get("stateVersion").is_some()
        && (!count(doc, "stateVersion") || crate::uimigrate::newer_than_known(doc))
    {
        return false;
    }
    let defaults = crate::uischema::defaults();
    pairs.is_empty() || pairs.iter().any(|(key, _)| defaults.get(key).is_some())
}
