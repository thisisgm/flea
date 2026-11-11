//! The backend's `plugins` request and its two answers, written by src/backend/plugins.rs and read by every front end.
use super::manifest::{Invalid, Manifest};
use super::schema::{self, Field, Kind, Value};
use crate::json::escape;
use crate::jsondoc::{self, Json};

pub const LISTING: &str = "plugins";
pub const RESULT: &str = "pluginop";
// Every verb the request takes; the CLI's `flea plugin <verb>` uses the same words.
pub const OPS: [&str; 8] = ["list", "enable", "disable", "set", "add", "update", "remove", "validate"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    // Ships inside Flea's package: no Update and no Remove.
    Builtin,
    Git(String),
    Folder(String),
    // Put in the plugins folder by hand; it can be removed but has nothing to update from.
    Unknown,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plugin {
    pub manifest: Manifest,
    pub source: Source,
    pub enabled: bool,
    // Why the plugin cannot run, when it cannot; its row then shows its state word in error ink.
    pub failure: Option<Invalid>,
    pub mark_path: String,
    pub settings: Vec<(String, Value)>,
    // The Shelf's page holds the shelf group and its pins rather than a schema.
    pub shelf: Option<super::shelf::Input>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    pub id: u64,
    pub plugins: Vec<Plugin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub id: u64,
    pub op: String,
    pub plugin: String,
    pub error: Option<Invalid>,
    // The plugin's version after an add or update, for the Update row's "Up to date" word.
    pub version: String,
}

// A wire line is one line, so this writes no whitespace; jsondoc::render is the pretty one ui.json uses.
pub fn compact(value: &Json) -> String {
    match value {
        Json::Null => "null".into(),
        Json::Bool(b) => b.to_string(),
        Json::Num(n) => n.clone(),
        Json::Str(s) => format!("\"{}\"", escape(s)),
        Json::Arr(items) => format!("[{}]", items.iter().map(compact).collect::<Vec<_>>().join(",")),
        Json::Obj(pairs) => format!(
            "{{{}}}",
            pairs.iter().map(|(k, v)| format!("\"{}\":{}", escape(k), compact(v))).collect::<Vec<_>>().join(",")
        ),
    }
}

fn s(text: &str) -> Json {
    Json::Str(text.into())
}

fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn strings(items: &[String]) -> Json {
    Json::Arr(items.iter().map(|i| s(i)).collect())
}

fn num(n: i64) -> Json {
    Json::Num(n.to_string())
}

// The field in its manifest shape, so a reader rebuilds it through schema::field and its checks.
pub fn field_json(field: &Field) -> Json {
    let mut pairs = vec![("key", s(&field.key)), ("label", s(&field.label)), ("hint", s(&field.hint))];
    match &field.kind {
        Kind::Boolean { default } => pairs.extend([("type", s("boolean")), ("defaultValue", Json::Bool(*default))]),
        Kind::Integer { min, max, step, default, unit } => pairs.extend([
            ("type", s("integer")), ("min", num(*min)), ("max", num(*max)), ("step", num(*step)),
            ("defaultValue", num(*default)), ("unit", s(unit)),
        ]),
        Kind::Text { default, placeholder } => {
            pairs.extend([("type", s("string")), ("defaultValue", s(default)), ("placeholder", s(placeholder))])
        }
        Kind::Choice { options, default } => pairs.extend([
            ("type", s("choice")),
            ("options", Json::Arr(options.iter().map(|(v, l)| obj(vec![("value", s(v)), ("label", s(l))])).collect())),
            ("defaultValue", s(default)),
        ]),
    }
    obj(pairs)
}

fn source_json(source: &Source) -> (Json, Json) {
    match source {
        Source::Builtin => (s("builtin"), s("")),
        Source::Git(origin) => (s("git"), s(origin)),
        Source::Folder(origin) => (s("folder"), s(origin)),
        Source::Unknown => (s(""), s("")),
    }
}

fn shelf_json(input: &super::shelf::Input) -> Json {
    let pins = input.pins.iter().map(|p| obj(vec![("path", s(&p.path)), ("folder", Json::Bool(p.folder)), ("missing", Json::Bool(p.missing))]));
    obj(vec![
        ("enabled", Json::Bool(input.enabled)), ("bar", Json::Bool(input.bar)), ("rail", s(&input.rail)),
        ("screenshots", Json::Bool(input.screenshots)), ("recordings", Json::Bool(input.recordings)),
        ("recent", num(input.recent)), ("pins", Json::Arr(pins.collect())),
    ])
}

// The pins ride the same shape the pile file gives them, so the one reader in shelf.rs reads both.
fn shelf_input(doc: &Json) -> super::shelf::Input {
    let pins: Vec<Json> = doc.get("pins").and_then(Json::as_array).unwrap_or(&[]).iter().map(|p| {
        let mut pairs = p.as_object().map(<[(String, Json)]>::to_vec).unwrap_or_default();
        pairs.push(("pinned".into(), Json::Bool(true)));
        Json::Obj(pairs)
    }).collect();
    let pile = compact(&obj(vec![("items", Json::Arr(pins))]));
    super::shelf::input(&obj(vec![("shelf", doc.clone())]), super::shelf::pinned(&pile))
}

fn plugin_json(p: &Plugin) -> Json {
    let m = &p.manifest;
    let (source, origin) = source_json(&p.source);
    let (failure, detail) = match &p.failure {
        Some(f) => (s(f.code), s(&f.detail)),
        None => (s(""), s("")),
    };
    obj(vec![
        ("id", s(&m.id)), ("name", s(&m.name)), ("version", s(&m.version)), ("author", s(&m.author)),
        ("license", s(&m.license)), ("description", s(&m.description)), ("kinds", strings(&m.kinds)),
        ("glyph", s(&m.glyph)), ("mark", s(&p.mark_path)), ("access", strings(&m.access)),
        ("source", source), ("origin", origin), ("enabled", Json::Bool(p.enabled)),
        ("failure", failure), ("detail", detail),
        ("schema", Json::Arr(m.schema.iter().map(field_json).collect())),
        ("blocks", Json::Obj(m.blocks.clone())),
        ("settings", Json::Obj(p.settings.iter().map(|(k, v)| (k.clone(), v.to_json())).collect())),
        ("shelf", p.shelf.as_ref().map_or(Json::Null, shelf_json)),
    ])
}

pub fn listing_line(listing: &Listing) -> String {
    compact(&obj(vec![
        ("t", s(LISTING)),
        ("id", Json::Num(listing.id.to_string())),
        ("plugins", Json::Arr(listing.plugins.iter().map(plugin_json).collect())),
    ]))
}

pub fn outcome_line(o: &Outcome) -> String {
    let (code, detail) = match &o.error {
        Some(e) => (e.code, e.detail.as_str()),
        None => ("", ""),
    };
    compact(&obj(vec![
        ("t", s(RESULT)), ("id", Json::Num(o.id.to_string())), ("op", s(&o.op)), ("plugin", s(&o.plugin)),
        ("ok", Json::Bool(o.error.is_none())), ("code", s(code)), ("detail", s(detail)), ("version", s(&o.version)),
    ]))
}

fn text(doc: &Json, key: &str) -> Result<String, String> {
    doc.get(key).and_then(Json::as_str).map(str::to_string).ok_or_else(|| format!("{key} must be a string"))
}

fn text_list(doc: &Json, key: &str) -> Result<Vec<String>, String> {
    let items = doc.get(key).and_then(Json::as_array).ok_or_else(|| format!("{key} must be an array"))?;
    items.iter().map(|i| i.as_str().map(str::to_string).ok_or_else(|| format!("{key} holds a non-string"))).collect()
}

fn reply_id(doc: &Json) -> Result<u64, String> {
    match doc.get("id").and_then(Json::as_f64) {
        Some(n) if n >= 0.0 && n.fract() == 0.0 && n < 9.0e15 => Ok(n as u64),
        _ => Err("id must be a whole number".into()),
    }
}

// The failure code is checked against the codes the backend writes, so a stray word never becomes a sentence key.
pub const FAILURE_CODES: [&str; 14] = [
    "json", "schema_version", "missing", "id", "reserved", "kind", "block", "run", "field", "long",
    "start", "exec", "manifest", "duplicate",
];

fn failure(code: &str, detail: String) -> Result<Option<Invalid>, String> {
    if code.is_empty() {
        return Ok(None);
    }
    let known = FAILURE_CODES.iter().chain(super::ERROR_CODES.iter()).find(|c| **c == code).ok_or("unknown failure code")?;
    Ok(Some(Invalid::new(known, detail)))
}

fn plugin(doc: &Json) -> Result<Plugin, String> {
    let schema: Vec<Field> = doc
        .get("schema")
        .and_then(Json::as_array)
        .ok_or("schema must be an array")?
        .iter()
        .map(|f| schema::field(f).map_err(|e| format!("bad field {}", e.detail)))
        .collect::<Result<_, _>>()?;
    let settings = schema::settings(&schema, doc.get("settings"));
    let origin = text(doc, "origin")?;
    let source = match text(doc, "source")?.as_str() {
        "builtin" => Source::Builtin,
        "git" => Source::Git(origin),
        "folder" => Source::Folder(origin),
        "" => Source::Unknown,
        _ => return Err("unknown source".into()),
    };
    let manifest = Manifest {
        id: text(doc, "id")?, name: text(doc, "name")?, version: text(doc, "version")?, author: text(doc, "author")?,
        license: text(doc, "license")?, description: text(doc, "description")?, kinds: text_list(doc, "kinds")?,
        run: String::new(), mark: String::new(), glyph: text(doc, "glyph")?, access: text_list(doc, "access")?, schema,
        blocks: doc.get("blocks").and_then(Json::as_object).unwrap_or(&[]).to_vec(),
    };
    Ok(Plugin {
        manifest, source,
        enabled: doc.get("enabled").and_then(Json::as_bool).ok_or("enabled must be a boolean")?,
        failure: failure(&text(doc, "failure")?, text(doc, "detail")?)?,
        mark_path: text(doc, "mark")?,
        settings,
        shelf: doc.get("shelf").filter(|v| v.as_object().is_some()).map(shelf_input),
    })
}

pub enum Reply {
    Listing(Listing),
    Outcome(Outcome),
}

// Sample input: {"t":"plugins","id":3,"plugins":[]} or {"t":"pluginop","id":3,"op":"enable","plugin":"a.b","ok":true,...}
pub fn parse(line: &str) -> Result<Reply, String> {
    let doc = jsondoc::parse(line)?;
    match doc.get("t").and_then(Json::as_str) {
        Some(LISTING) => Ok(Reply::Listing(Listing {
            id: reply_id(&doc)?,
            plugins: doc.get("plugins").and_then(Json::as_array).ok_or("plugins must be an array")?.iter().map(plugin).collect::<Result<_, _>>()?,
        })),
        Some(RESULT) => {
            let ok = doc.get("ok").and_then(Json::as_bool).ok_or("ok must be a boolean")?;
            let error = if ok { None } else { failure(&text(&doc, "code")?, text(&doc, "detail")?)?.or(Some(Invalid::new("manifest", ""))) };
            Ok(Reply::Outcome(Outcome {
                id: reply_id(&doc)?, op: text(&doc, "op")?, plugin: text(&doc, "plugin")?, error, version: text(&doc, "version")?,
            }))
        }
        _ => Err("not a plugins reply".into()),
    }
}

// Sample output: {"c":"plugins","op":"enable","id":4,"plugin":"mtorvik.checksums"}
pub fn request_line(id: u64, op: &str, plugin: &str, extra: Vec<(&str, Json)>) -> String {
    let mut pairs = vec![("c", s("plugins")), ("op", s(op)), ("id", Json::Num(id.to_string()))];
    if !plugin.is_empty() {
        pairs.push(("plugin", s(plugin)));
    }
    pairs.extend(extra);
    compact(&obj(pairs))
}
