//! The trashbrowse wire: held rows and replies read from one parsed line, and request lines written compactly.
use crate::json::escape;
use crate::jsondoc::Json;

/// The reviewed image a row's thumbnail is made from; every field is opaque and copied back exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thumbnail {
    pub path: String,
    pub provider: String,
    pub identity: String,
    pub mtime: i64,
}

/// One held Trash row as src/backend/trashbrowse.rs writes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrashRow {
    pub uri: String,
    pub original: String,
    pub size: u64,
    pub deleted: Option<String>,
    pub directory: bool,
    pub icon: String,
    pub identity: String,
    pub bulk_selected: bool,
    pub storage_class: Option<String>,
    pub thumbnail: Option<Thumbnail>,
}

pub fn text(value: &Json, key: &str) -> Option<String> {
    value.get(key).and_then(Json::as_str).map(String::from)
}

pub fn count(value: &Json, key: &str) -> Option<u64> {
    value.get(key).and_then(Json::as_f64).filter(|n| *n >= 0. && n.fract() == 0.).map(|n| n as u64)
}

pub fn flag(value: &Json, key: &str) -> bool {
    value.get(key).and_then(Json::as_bool).unwrap_or(false)
}

pub fn texts(value: &Json, key: &str) -> Vec<String> {
    value.get(key).and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect()
}

impl Thumbnail {
    fn read(value: &Json) -> Option<Self> {
        Some(Self {
            path: text(value, "path")?,
            provider: text(value, "provider")?,
            identity: text(value, "identity")?,
            mtime: value.get("mtime").and_then(Json::as_f64)? as i64,
        })
    }
}

impl TrashRow {
    /// A row without a uri or an original is no row; the window that held it fails closed.
    pub fn read(value: &Json) -> Option<Self> {
        Some(Self {
            uri: text(value, "uri")?,
            original: text(value, "original")?,
            size: count(value, "size").unwrap_or(0),
            deleted: text(value, "deleted"),
            directory: flag(value, "directory"),
            icon: text(value, "icon").unwrap_or_default(),
            identity: text(value, "identity").unwrap_or_default(),
            bulk_selected: flag(value, "bulkSelected"),
            storage_class: match value.get("storageClass") {
                None => Some(String::new()),
                Some(class) => class.as_str().map(String::from),
            },
            thumbnail: value.get("thumbnail").and_then(Thumbnail::read),
        })
    }

    /// The name the row draws: the original's last segment.
    pub fn name(&self) -> &str {
        self.original.rsplit('/').next().unwrap_or("")
    }

    /// TrashThumbs.qml: a row the backend described an image for, unless its store is null (unclassified).
    pub fn thumbnailable(&self) -> bool {
        self.thumbnail.is_some() && self.storage_class.is_some()
    }
}

/// A trashbrowse reply's envelope; the rest stays in the parsed value for the op that reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub id: u64,
    pub op: String,
    pub ok: bool,
    pub stale: bool,
    pub error: Option<String>,
    pub body: Json,
}

impl Reply {
    pub fn read(body: Json) -> Option<Self> {
        if body.get("t").and_then(Json::as_str) != Some("trashbrowse") {
            return None;
        }
        Some(Self {
            id: count(&body, "id")?,
            op: text(&body, "op")?,
            ok: flag(&body, "ok"),
            stale: flag(&body, "stale"),
            error: text(&body, "error").filter(|e| !e.is_empty()),
            body,
        })
    }

    pub fn rows(&self) -> Option<Vec<TrashRow>> {
        self.body.get("rows").and_then(Json::as_array).unwrap_or(&[]).iter().map(TrashRow::read).collect()
    }
}

/// One compact request line, so the backend reads it as one wire line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line(String);

impl Line {
    pub fn new(command: &str) -> Self {
        Self(format!(r#"{{"c":"{}""#, escape(command)))
    }
    pub fn text(mut self, key: &str, value: &str) -> Self {
        self.0.push_str(&format!(r#","{key}":"{}""#, escape(value)));
        self
    }
    pub fn number(mut self, key: &str, value: impl std::fmt::Display) -> Self {
        self.0.push_str(&format!(r#","{key}":{value}"#));
        self
    }
    pub fn flag(mut self, key: &str, value: bool) -> Self {
        self.0.push_str(&format!(r#","{key}":{value}"#));
        self
    }
    pub fn texts(mut self, key: &str, values: &[String]) -> Self {
        self.0.push_str(&format!(r#","{key}":{}"#, crate::json::string_array(values)));
        self
    }
    pub fn numbers(mut self, key: &str, values: &[u64]) -> Self {
        let joined = values.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
        self.0.push_str(&format!(r#","{key}":[{joined}]"#));
        self
    }
    pub fn finish(mut self) -> String {
        self.0.push('}');
        self.0
    }
}
