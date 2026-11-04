//! Validated plugin-relative listings projected into Flea's normal viewport protocol.
use flea::{jsondoc::Json, protocol::RowFacts, ui_logic_mn1::build};
use std::collections::{HashMap, HashSet};

pub(super) fn text<'a>(doc: &'a Json, key: &str) -> &'a str {
    doc.get(key).and_then(Json::as_str).unwrap_or("")
}
pub(super) fn num(doc: &Json, key: &str) -> u64 {
    doc.get(key)
        .and_then(Json::as_f64)
        .filter(|n| n.is_finite() && *n >= 0. && n.fract() == 0.)
        .unwrap_or(0.) as u64
}
pub(super) fn object(pairs: &[(&str, Json)]) -> Json {
    Json::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}
pub(super) fn string(value: impl Into<String>) -> Json {
    Json::Str(value.into())
}
pub(super) fn number(value: impl ToString) -> Json {
    Json::Num(value.to_string())
}
pub(super) fn path(uri: &str) -> Option<(&str, &str)> {
    flea::plugin_runtime::protocol::place_path(uri)
}

#[derive(Clone)]
pub(super) struct Snapshot {
    pub path: String,
    pub paths: Vec<String>,
    pub listing: usize,
}
#[derive(Default)]
pub(super) struct Model {
    pub path: String,
    pub listing: usize,
    pub entries: Vec<RowFacts>,
    pub rows: Vec<RowFacts>,
    pub snapshots: HashMap<u64, Snapshot>,
    pub delete_tokens: HashSet<u64>,
    pub order: String,
    pub reverse: bool,
    pub hidden: bool,
    pub folders_first: bool,
    pub first: usize,
}
impl Model {
    pub fn accept(&mut self, message: &Json, request: &Json) -> Result<Vec<Json>, String> {
        flea::plugin_runtime::protocol::response(message, "list")?;
        let entries = message
            .get("entries")
            .and_then(Json::as_array)
            .ok_or("invalid place listing")?;
        let mut rows = Vec::new();
        for entry in entries {
            let name = text(entry, "name");
            if name.chars().any(|c| c.is_control() || matches!(c, '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')) || name.len() > super::limits::NAME_BYTES || name.contains('\\') {
                return Err("invalid plugin entry name".into());
            }
            let dir = match text(entry, "kind") {
                "dir" => true,
                "file" => false,
                _ => return Err("invalid plugin entry kind".into()),
            };
            let size = match entry.get("size") {
                None => 0,
                Some(Json::Num(n)) => n.parse::<u64>().map_err(|_| "invalid plugin entry size")?,
                _ => return Err("invalid plugin entry size".into()),
            };
            let mtime = match entry.get("mtime") {
                None => 0,
                Some(Json::Num(n)) => n.parse::<i64>().map_err(|_| "invalid plugin entry mtime")?,
                Some(Json::Str(s)) => flea::ui_logic_pl4::recent::stamp_seconds(s)
                    .ok_or("invalid plugin entry mtime")?,
                _ => return Err("invalid plugin entry mtime".into()),
            };
            rows.push(RowFacts {
                name: name.into(),
                dir,
                size,
                mtime,
                mode: if dir {
                    build::S_IFDIR | 0o755
                } else {
                    build::S_IFREG | 0o644
                },
                icon: if dir { "folder" } else { "file" }.into(),
                real_modes: false,
                ..Default::default()
            });
        }
        self.entries = rows;
        self.settings(request);
        self.publish(request)
    }
    pub fn settings(&mut self, request: &Json) {
        self.order = text(request, "by").into();
        self.reverse = request.get("desc").and_then(Json::as_bool).unwrap_or(false);
        self.hidden = request
            .get("hidden")
            .and_then(Json::as_bool)
            .unwrap_or(self.hidden);
        self.folders_first = request
            .get("foldersFirst")
            .and_then(Json::as_bool)
            .unwrap_or(true);
    }
    pub fn publish(&mut self, request: &Json) -> Result<Vec<Json>, String> {
        self.rows = self
            .entries
            .iter()
            .filter(|r| self.hidden || !r.name.starts_with('.'))
            .cloned()
            .collect();
        self.rows.sort_by(|a, b| {
            let group = if self.folders_first {
                b.dir.cmp(&a.dir)
            } else {
                std::cmp::Ordering::Equal
            };
            let order = match self.order.as_str() {
                "size" => a.size.cmp(&b.size),
                "mtime" => a.mtime.cmp(&b.mtime),
                "kind" => a.dir.cmp(&b.dir),
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
            .then(a.name.cmp(&b.name));
            group.then(if self.reverse { order.reverse() } else { order })
        });
        self.listing = self
            .listing
            .checked_add(1)
            .ok_or("plugin listing exhausted")?;
        self.snapshots.clear();
        self.delete_tokens.clear();
        let first = request
            .get("first")
            .map_or(self.first.max(1), |_| num(request, "first") as usize);
        self.first = first;
        let select = text(request, "select");
        let start = self
            .rows
            .iter()
            .position(|r| format!("{}/{}", self.path.trim_end_matches('/'), r.name) == select)
            .filter(|at| *at >= first)
            .unwrap_or(0);
        Ok(vec![
            object(&[
                ("t", string("listed")),
                ("path", string(&self.path)),
                ("n", number(self.rows.len())),
            ]),
            self.window(start, first.max(1)),
        ])
    }
    pub fn window(&self, start: usize, count: usize) -> Json {
        let start = start.min(self.rows.len());
        let rows = self
            .rows
            .iter()
            .skip(start)
            .take(count.min(super::limits::WINDOW_ROWS))
            .map(|r| {
                object(&[
                    ("n", string(&r.name)),
                    ("d", Json::Bool(r.dir)),
                    ("s", number(r.size)),
                    ("m", number(r.mtime)),
                    ("p", number(r.mode)),
                    ("i", string(&r.icon)),
                    ("x", Json::Bool(false)),
                ])
            })
            .collect();
        object(&[
            ("t", string("rows")),
            ("start", number(start)),
            ("listing", number(self.listing)),
            ("rows", Json::Arr(rows)),
        ])
    }
    pub fn targets(&self, request: &Json) -> Result<Vec<String>, String> {
        if let Some(paths) = request.get("paths").and_then(Json::as_array) {
            if paths.is_empty() || paths.len() > super::limits::SELECTION {
                return Err("no plugin operation paths".into());
            }
            return paths
                .iter()
                .map(|p| {
                    p.as_str()
                        .filter(|p| !p.chars().any(char::is_control))
                        .map(str::to_owned)
                        .ok_or("invalid plugin path".into())
                })
                .collect();
        }
        if num(request, "listing") as usize != self.listing {
            return Err("plugin listing changed".into());
        }
        let rows = request
            .get("rows")
            .and_then(Json::as_array)
            .ok_or("missing plugin rows")?;
        if rows.is_empty() || rows.len() > super::limits::SELECTION {
            return Err("no plugin operation rows".into());
        }
        rows.iter()
            .map(|r| {
                let index = r
                    .as_f64()
                    .filter(|n| *n >= 0. && n.fract() == 0.)
                    .ok_or("invalid plugin row")? as usize;
                let row = self.rows.get(index).ok_or("plugin row no longer exists")?;
                Ok(format!("{}/{}", self.path.trim_end_matches('/'), row.name))
            })
            .collect()
    }
}
