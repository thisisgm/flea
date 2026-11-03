//! A plugin's manifest.json, schemaVersion 1, read and checked the way `flea plugin validate` reports it.
use super::schema::{self, Field};
use crate::jsondoc::{self, Json};

pub const SCHEMA_VERSION: f64 = 1.0;
// Each kind is a place Flea already draws, paired with the block that describes it.
pub const KINDS: [(&str, &str); 6] = [
    ("send-target", "sendTargets"),
    ("menu-action", "menuActions"),
    ("rail-section", "railSection"),
    ("place", "place"),
    ("previewer", "previewer"),
    ("shelf", "shelf"),
];
pub const RESERVED_PREFIX: &str = "flea.";
const MAX_ID: usize = 64;
const MAX_VERSION: usize = 64;
const MAX_TEXT: usize = 512;
// A hand-written manifest is a few kilobytes; anything far past that is not one.
pub const MAX_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub description: String,
    pub kinds: Vec<String>,
    pub run: String,
    pub mark: String,
    pub glyph: String,
    pub access: Vec<String>,
    pub schema: Vec<Field>,
    pub blocks: Vec<(String, Json)>,
}

// The machine identity of a refusal: `code` selects the plugins.invalid.* sentence and `detail` is its one argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invalid {
    pub code: &'static str,
    pub detail: String,
}

impl Invalid {
    pub fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self { code, detail: detail.into() }
    }

    pub fn key(&self) -> String {
        super::sentence_key(self.code)
    }
}

// Sample input: "yourname.optimize". Lowercase segments of letters, digits, '-' or '_', at least two of them.
pub fn is_id(id: &str) -> bool {
    let segments: Vec<&str> = id.split('.').collect();
    id.len() <= MAX_ID
        && segments.len() >= 2
        && segments.iter().all(|s| {
            !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        })
}

// A path inside the plugin's own folder: relative, and never climbing out of it.
pub fn is_inner_path(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains('\0') && path.split('/').all(|part| part != "..")
}

fn text<'a>(doc: &'a Json, key: &'static str, required: bool) -> Result<&'a str, Invalid> {
    match doc.get(key) {
        Some(Json::Str(s)) if s.chars().count() <= MAX_TEXT => Ok(s.trim_end_matches('\n')),
        Some(Json::Str(_)) => Err(Invalid::new("long", key)),
        None if !required => Ok(""),
        _ => Err(Invalid::new("missing", key)),
    }
}

fn strings(doc: &Json, key: &'static str) -> Result<Vec<String>, Invalid> {
    match doc.get(key) {
        None => Ok(Vec::new()),
        Some(Json::Arr(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_string).ok_or_else(|| Invalid::new("missing", key)))
            .collect(),
        Some(_) => Err(Invalid::new("missing", key)),
    }
}

// builtin marks a manifest Flea ships: its id may take the reserved prefix and it runs inside Flea, so it names no executable.
pub fn parse(text_in: &str, builtin: bool) -> Result<Manifest, Invalid> {
    if text_in.len() > MAX_BYTES {
        return Err(Invalid::new("json", "manifest.json"));
    }
    let doc = jsondoc::parse(text_in).map_err(|e| Invalid::new("json", e))?;
    if doc.as_object().is_none() {
        return Err(Invalid::new("json", "manifest.json"));
    }
    if doc.get("schemaVersion").and_then(Json::as_f64) != Some(SCHEMA_VERSION) {
        return Err(Invalid::new("schema_version", "schemaVersion"));
    }
    let id = text(&doc, "id", true)?;
    if !is_id(id) {
        return Err(Invalid::new("id", id));
    }
    if id.starts_with(RESERVED_PREFIX) && !builtin {
        return Err(Invalid::new("reserved", id));
    }
    let name = text(&doc, "name", true)?;
    let version = text(&doc, "version", true)?;
    if version.chars().count() > MAX_VERSION {
        return Err(Invalid::new("long", "version"));
    }
    let author = text(&doc, "author", true)?;
    let description = text(&doc, "description", true)?;
    if name.trim().is_empty() {
        return Err(Invalid::new("missing", "name"));
    }
    let kinds = strings(&doc, "kinds")?;
    if kinds.is_empty() {
        return Err(Invalid::new("missing", "kinds"));
    }
    for kind in &kinds {
        let block = KINDS.iter().find(|(k, _)| k == kind).map(|(_, b)| *b).ok_or_else(|| Invalid::new("kind", kind.as_str()))?;
        if doc.get(block).is_none() {
            return Err(Invalid::new("block", block));
        }
    }
    let run = match doc.get("entryPoints").and_then(|e| e.get("run")).and_then(Json::as_str) {
        Some(run) if is_inner_path(run) => run,
        Some(run) => return Err(Invalid::new("run", run)),
        None if builtin => "",
        None => return Err(Invalid::new("missing", "entryPoints.run")),
    };
    let mark = text(&doc, "mark", false)?;
    if !mark.is_empty() && !is_inner_path(mark) {
        return Err(Invalid::new("run", mark));
    }
    let schema = match doc.get("schema") {
        None => Vec::new(),
        Some(Json::Arr(fields)) => fields.iter().map(schema::field).collect::<Result<_, _>>()?,
        Some(_) => return Err(Invalid::new("missing", "schema")),
    };
    for (at, field) in schema.iter().enumerate() {
        if schema[..at].iter().any(|f: &Field| f.key == field.key) {
            return Err(Invalid::new("field", field.key.as_str()));
        }
    }
    let blocks = super::contributions::validate(&doc, &kinds)?;
    Ok(Manifest {
        id: id.into(),
        name: name.into(),
        version: version.into(),
        author: author.into(),
        license: text(&doc, "license", false)?.into(),
        description: description.into(),
        kinds,
        run: run.into(),
        mark: mark.into(),
        glyph: glyph(&doc),
        access: strings(&doc, "access")?,
        schema,
        blocks,
    })
}

impl Manifest {
    pub fn block(&self, kind: &str) -> Option<&Json> {
        let (_, name) = KINDS.iter().find(|(name, _)| *name == kind)?;
        self.kinds.iter().any(|k| k == kind).then(|| self.blocks.iter().find(|(key, _)| key == name).map(|(_, v)| v)).flatten()
    }
}

// The row mark a plugin without its own SVG names: the first glyph any of its blocks gives.
fn glyph(doc: &Json) -> String {
    let named = |value: &Json| value.get("glyph").and_then(Json::as_str).map(str::to_string);
    for (_, block) in KINDS {
        match doc.get(block) {
            Some(Json::Arr(items)) => {
                if let Some(found) = items.iter().find_map(named) {
                    return found;
                }
            }
            Some(value) => {
                if let Some(found) = named(value) {
                    return found;
                }
            }
            None => {}
        }
    }
    String::new()
}
