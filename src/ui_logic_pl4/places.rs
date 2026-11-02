//! The PLACES and FAVORITES data the rail and the query share: parsing, validation and the open decision.
use super::rail::{FavouriteError, Group, Kind, RailEntry, RecordError};
use super::{decode_path, has_control, js_trim, leaf, normalize, userdirs};
use crate::jsondoc::Json;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bookmark {
    pub path: String,
    pub label: String,
}

// Sample input: "file:///home/gm/Downloads Downloads"; an smb:// or sftp:// line is a Location, never a Favorite.
pub fn bookmarks(body: &str) -> Vec<Bookmark> {
    let mut out = Vec::new();
    for line in body.split('\n').map(js_trim) {
        let Some(rest) = line.strip_prefix("file://") else {
            continue;
        };
        let (uri, label) = rest
            .split_once(' ')
            .map_or((rest, ""), |(u, l)| (u, js_trim(l)));
        let path = decode_path(uri);
        let label = if label.is_empty() {
            leaf(&path).to_owned()
        } else {
            label.to_owned()
        };
        out.push(Bookmark { path, label });
    }
    out
}

// The merged FAVORITES rows: Home first and always, then XDG dirs and bookmarks in file order, a repeat keeping its first place.
pub fn favorites(
    home: &str,
    dirs: &str,
    marks: &str,
    glyph_for: impl Fn(&str) -> &'static str,
) -> Vec<RailEntry> {
    let mut favs = vec![RailEntry {
        group: Group::Favorite,
        kind: Kind::Favorite,
        label_key: "places.row.home",
        path: home.to_owned(),
        glyph: glyph_for("Home"),
        ..RailEntry::default()
    }];
    let found = userdirs::user_dirs(dirs, home)
        .into_iter()
        .map(|dir| (dir.path, dir.label, dir.label_key))
        .chain(
            bookmarks(marks)
                .into_iter()
                .map(|mark| (mark.path, mark.label, "")),
        );
    for (path, label, label_key) in found {
        if favs.iter().any(|seen| seen.path == path) {
            continue;
        }
        favs.push(RailEntry {
            group: Group::Favorite,
            kind: Kind::Favorite,
            glyph: glyph_for(&label),
            label,
            label_key,
            path,
            ..RailEntry::default()
        });
    }
    favs
}

fn uri_of(line: &str) -> &str {
    line.split_once(' ').map_or(line, |(uri, _)| uri)
}

fn appended(body: &str, line: &str) -> String {
    let mut out = body.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out + line + "\n"
}

// Every line naming path takes the new label; none does, and one line is appended. See AGENTS.md "Places.relabel".
pub fn relabel(body: &str, path: &str, name: &str) -> String {
    let trimmed: String = name.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    let trimmed = js_trim(&trimmed);
    if trimmed.is_empty() {
        return body.to_owned();
    }
    let target = normalize(path);
    let mut found = false;
    let lines: Vec<String> = body
        .split('\n')
        .map(|raw| {
            let line = js_trim(raw);
            if !line.is_empty() && normalize(uri_of(line)) == target {
                found = true;
                format!("{} {}", uri_of(line), trimmed)
            } else {
                raw.to_owned()
            }
        })
        .collect();
    if found {
        lines.join("\n")
    } else {
        appended(body, &format!("{target} {trimmed}"))
    }
}

// The saved line old_uri names is rewritten in place to new_uri; nothing matching appends. PR 21.
pub fn replace(body: &str, old_uri: &str, new_uri: &str, label: &str) -> String {
    let next = normalize(new_uri);
    if next.is_empty() {
        return body.to_owned();
    }
    let name: String = label.chars().filter(|c| *c != '\r' && *c != '\n').collect();
    let name = match js_trim(&name) {
        "" => leaf(&next).to_owned(),
        kept => kept.to_owned(),
    };
    let target = normalize(old_uri);
    let mut found = false;
    let lines: Vec<String> = body
        .split('\n')
        .map(|raw| {
            let line = js_trim(raw);
            if !line.is_empty() && !target.is_empty() && normalize(uri_of(line)) == target {
                found = true;
                format!("{next} {name}")
            } else {
                raw.to_owned()
            }
        })
        .collect();
    if found {
        lines.join("\n")
    } else {
        appended(body, &format!("{next} {name}"))
    }
}

pub const WIDTH_STOPS: [i64; 4] = [160, 192, 224, 256];

// Sample input: 231 snaps to 224; a value that is not a finite number keeps the default 192.
pub fn sidebar_width(value: Option<f64>) -> i64 {
    let Some(value) = value.filter(|v| v.is_finite()) else {
        return 192;
    };
    WIDTH_STOPS
        .into_iter()
        .fold(WIDTH_STOPS[0], |nearest, stop| {
            if (stop as f64 - value).abs() < (nearest as f64 - value).abs() {
                stop
            } else {
                nearest
            }
        })
}

const SCHEMES: [&str; 8] = [
    "smb://", "sftp://", "ftp://", "dav://", "davs://", "afp://", "nfs://", "file://",
];

// Why a stored favourite record cannot stand, None when it can.
pub fn record_error(record: &Json) -> Option<RecordError> {
    if record.as_object().is_none() {
        return Some(RecordError::InvalidRecord);
    }
    if !record
        .get("label")
        .and_then(Json::as_str)
        .is_some_and(|label| !js_trim(label).is_empty())
    {
        return Some(RecordError::LabelBlank);
    }
    let Some(path) = record
        .get("path")
        .and_then(Json::as_str)
        .filter(|path| !has_control(path))
    else {
        return Some(RecordError::InvalidPath);
    };
    if path.starts_with('/') || path == "~" || path.starts_with("~/") {
        return None;
    }
    if SCHEMES
        .iter()
        .any(|scheme| path.len() > scheme.len() && path.starts_with(scheme))
    {
        return None;
    }
    Some(RecordError::NotAPlace)
}

// JSON.stringify's one-line form, the label an invalid record keeps so it stays identifiable.
pub fn stringify(value: &Json) -> String {
    match value {
        Json::Arr(items) => format!(
            "[{}]",
            items.iter().map(stringify).collect::<Vec<_>>().join(",")
        ),
        Json::Obj(pairs) => {
            let members: Vec<String> = pairs
                .iter()
                .map(|(key, value)| {
                    format!("\"{}\":{}", crate::json::escape(key), stringify(value))
                })
                .collect();
            format!("{{{}}}", members.join(","))
        }
        scalar => crate::jsondoc::render_at(scalar, 0),
    }
}

// One row per stored record, invalid ones kept and marked so the operator can find and remove them.
pub fn stored_entries(records: &[Json], home: &str) -> Vec<RailEntry> {
    records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let label = match record.get("label").and_then(Json::as_str) {
                Some(label) => label.to_owned(),
                None => stringify(record),
            };
            let path = record.get("path").and_then(Json::as_str).unwrap_or("");
            let resolved = if path == "~" {
                home.to_owned()
            } else if let Some(rest) = path.strip_prefix("~/") {
                format!("{home}/{rest}")
            } else {
                path.to_owned()
            };
            RailEntry {
                group: Group::Favourite,
                kind: Kind::Favourite,
                label_key: if label.is_empty() {
                    "places.favorite.invalid_label"
                } else {
                    ""
                },
                label,
                path: resolved,
                stored_path: path.to_owned(),
                error: record_error(record).map(FavouriteError::Record),
                original: Some(record.clone()),
                favourite_index: Some(index),
                glyph: if path.contains("://") {
                    "network"
                } else {
                    "folder"
                },
                ..RailEntry::default()
            }
        })
        .collect()
}

// Home and the XDG dirs, each a home row carrying its mark.
pub fn home_entries(
    home: &str,
    dirs: &str,
    glyph_for: impl Fn(&str) -> &'static str,
) -> Vec<RailEntry> {
    let lead = RailEntry {
        group: Group::Home,
        kind: Kind::Home,
        label_key: "places.row.home",
        path: home.to_owned(),
        glyph: glyph_for("Home"),
        ..RailEntry::default()
    };
    std::iter::once(lead)
        .chain(
            userdirs::user_dirs(dirs, home)
                .into_iter()
                .map(|dir| RailEntry {
                    group: Group::Home,
                    kind: Kind::Home,
                    glyph: glyph_for(&dir.label),
                    label: dir.label,
                    label_key: dir.label_key,
                    path: dir.path,
                    ..RailEntry::default()
                }),
        )
        .collect()
}
