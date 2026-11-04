use super::regfile;
use crate::jsondoc::{self, Json};
use crate::oflags::O_NOFOLLOW;
use std::fs::Metadata;
use std::io::Read;
use std::os::unix::{ffi::OsStringExt, fs::MetadataExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const MAX_RESOURCES: usize = 256;
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

fn candidate_path(folder: &Path, encoded: &str) -> Option<PathBuf> {
    let bytes = crate::paths::percent_decode_bytes(encoded);
    if bytes.is_empty() || bytes.contains(&0) { return None }
    let relative = PathBuf::from(std::ffi::OsString::from_vec(bytes));
    if relative.components().any(|c| !matches!(c, Component::Normal(_))) { return None }
    Some(folder.join(relative))
}

fn local_path(folder: &Path, encoded: &str) -> Option<PathBuf> {
    let path = candidate_path(folder, encoded)?;
    let resolved = path.canonicalize().ok()?;
    resolved.starts_with(folder).then_some(path)
}

fn stamp(folder: &Path, encoded: &str) -> Option<Metadata> {
    let meta = local_path(folder, encoded)?.metadata().ok()?;
    meta.is_file().then_some(meta)
}

fn number(value: impl ToString) -> Json { Json::Num(value.to_string()) }

fn fields(meta: &Metadata) -> Vec<Json> {
    vec![number(meta.len()), number(meta.mtime()), number(meta.mtime_nsec()), number(meta.ino()), number(meta.dev())]
}

// Sample input: HTML_RESOURCES {"resources":[{"url":"file:///page/sub/style.css","failed":false}],"overflow":false}.
pub fn record(text: &str, folder: &Path, started: SystemTime) -> Option<Vec<u8>> {
    if text.len() > MAX_RECORD_BYTES { return None }
    let document = jsondoc::parse(text).ok()?;
    if document.get("overflow")?.as_bool()? { return None }
    let resources = document.get("resources")?.as_array()?;
    if resources.len() > MAX_RESOURCES { return None }
    let mut rows = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for resource in resources {
        let url = resource.get("url")?.as_str()?;
        let failed = resource.get("failed")?.as_bool()?;
        let Some(path) = url.strip_prefix("file:///page/").or_else(|| url.strip_prefix("file://localhost/page/")) else { continue };
        let path = path.split(['?', '#']).next()?;
        let duplicate = !seen.insert(path);
        let candidate = candidate_path(folder, path)?;
        if candidate.canonicalize().is_ok_and(|resolved| !resolved.starts_with(folder)) {
            if !duplicate { rows.push(Json::Arr(vec![Json::Str(path.to_string()), Json::Str("blocked".into())])); }
            continue;
        }
        let meta = match candidate.metadata() {
            Ok(_) => stamp(folder, path)?,
            Err(error) if failed && error.kind() == std::io::ErrorKind::NotFound => {
                if !duplicate { rows.push(Json::Arr(vec![Json::Str(path.to_string()), Json::Str("absent".into())])); }
                continue;
            }
            Err(_) => return None,
        };
        let changed = SystemTime::UNIX_EPOCH + Duration::new(meta.ctime().max(0) as u64, meta.ctime_nsec().max(0) as u32);
        if meta.modified().ok()? > started || changed > started { return None }
        if duplicate { continue }
        let mut row = vec![Json::Str(path.to_string())];
        row.extend(fields(&meta));
        rows.push(Json::Arr(row));
    }
    let bytes = jsondoc::render(&Json::Obj(vec![("version".into(), number(1)), ("dependencies".into(), Json::Arr(rows))])).into_bytes();
    (bytes.len() <= MAX_RECORD_BYTES).then_some(bytes)
}

pub fn sidecar(png: &Path) -> PathBuf { png.with_extension("deps") }

// The rendered record up to its dependency list, which every version 1 record starts with.
fn own_header() -> String {
    let full = jsondoc::render(&Json::Obj(vec![("version".into(), number(1)), ("dependencies".into(), Json::Arr(Vec::new()))]));
    full[..full.find("\"dependencies\"").unwrap_or(0)].to_string()
}

pub(super) fn replaceable(png: &Path) -> bool {
    crate::persistdoc::replaceable(&sidecar(png), MAX_RECORD_BYTES as u64, own_header().as_bytes(), |bytes| {
        // Bytes that are no JSON at all are damage to this cache's own record, which 0.3.12 rendered again (html-engine F4).
        let Some(doc) = std::str::from_utf8(bytes).ok().and_then(|s| jsondoc::parse(s).ok()) else { return true };
        doc.as_object().is_some_and(|pairs| pairs.len() == 2)
            && doc.get("version") == Some(&number(1))
            && doc.get("dependencies").and_then(Json::as_array).is_some_and(|rows| {
                rows.len() <= MAX_RESOURCES && rows.iter().all(|row| {
                    row.as_array().is_some_and(|fields| {
                        fields.first().and_then(Json::as_str).is_some()
                            && ((fields.len() == 2 && matches!(fields[1].as_str(), Some("absent" | "blocked")))
                                || (fields.len() == 6 && fields[1..].iter().all(|v| v.as_f64().is_some())))
                    })
                })
            })
    })
}

pub fn unchanged(png: &Path, folder: &Path) -> bool {
    let Ok(file) = regfile::open_if_regular(&sidecar(png), O_NOFOLLOW) else { return false };
    let mut text = String::new();
    if file.take(MAX_RECORD_BYTES as u64 + 1).read_to_string(&mut text).is_err() || text.len() > MAX_RECORD_BYTES { return false }
    let Ok(document) = jsondoc::parse(&text) else { return false };
    if document.get("version") != Some(&number(1)) { return false }
    let Some(rows) = document.get("dependencies").and_then(Json::as_array) else { return false };
    if rows.len() > MAX_RESOURCES { return false }
    rows.iter().all(|row| {
        let Some(row) = row.as_array() else { return false };
        let Some(path) = row.first().and_then(Json::as_str) else { return false };
        // An absent row holds only while its file is still missing; any arrival is a miss.
        if row.len() == 2 && row[1] == Json::Str("absent".into()) {
            return candidate_path(folder, path).is_some_and(|candidate| candidate.metadata()
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound));
        }
        if row.len() == 2 && row[1] == Json::Str("blocked".into()) {
            return candidate_path(folder, path).and_then(|candidate| candidate.canonicalize().ok())
                .is_some_and(|resolved| !resolved.starts_with(folder));
        }
        if row.len() != 6 { return false }
        stamp(folder, path).is_some_and(|meta| row[1..] == fields(&meta))
    })
}

#[cfg(test)]
#[path = "htmldeps_tests.rs"]
mod tests;
