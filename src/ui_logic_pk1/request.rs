// The portal request as tools/flea-portal hands it in FLEA_PICKER, read the way ui/js/Picker.js reads it.
use crate::jsondoc::{self, Json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Open,
    Save,
    SaveFiles,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    pub label: String,
    pub globs: Vec<String>,
    pub mimes: Vec<String>,
}

// Every field defaulted, because a request that arrived short must still open a window.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub mode: Mode,
    pub title: String,
    pub app: String,
    pub accept: String,
    pub multiple: bool,
    pub directory: bool,
    pub hidden: Option<bool>,
    pub folder: String,
    pub file: String,
    pub name: String,
    pub files: Vec<Json>,
    pub filters: Vec<Filter>,
    pub current: String,
    pub current_index: i64,
}

// Text a surface shows: the caller's own words verbatim, or a catalog key the front end translates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Text {
    Caller(String),
    Key(&'static str),
}

// The primary answer's label; the count rides on it only for a several-file request holding more than one mark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptLabel {
    pub base: Text,
    pub count: Option<usize>,
}

// Sample input: "{\"mode\":\"save\",\"name\":\"a.txt\"}"; "not json" still answers an open request.
pub fn request(text: &str) -> Request {
    let read = jsondoc::parse(text).unwrap_or(Json::Null);
    let get = |key: &str| member(&read, key);
    let field = |key: &str| get(key).map(js_string).unwrap_or_default();
    let flag = |key: &str| get(key).and_then(Json::as_bool) == Some(true);
    let mode = match get("mode").and_then(Json::as_str) {
        Some("save") => Mode::Save,
        Some("savefiles") => Mode::SaveFiles,
        _ => Mode::Open,
    };
    let filters = match get("filters") {
        Some(Json::Arr(items)) => items.iter().map(filter).collect(),
        _ => Vec::new(),
    };
    Request {
        mode,
        title: field("title"),
        app: field("app"),
        accept: field("accept"),
        multiple: flag("multiple"),
        directory: flag("directory"),
        hidden: get("hidden").and_then(Json::as_bool),
        folder: field("folder"),
        file: field("file"),
        name: field("name"),
        files: match get("files") {
            Some(Json::Arr(items)) => items.clone(),
            _ => Vec::new(),
        },
        filters,
        current: field("current"),
        current_index: get("currentIndex")
            .and_then(Json::as_f64)
            .filter(|index| index.fract() == 0. && index.abs() < 1e15)
            .map_or(-1, |index| index as i64),
    }
}

// JSON.parse keeps the last of duplicate members, so lookups read from the end.
fn member<'a>(value: &'a Json, key: &str) -> Option<&'a Json> {
    value
        .as_object()?
        .iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, item)| item)
}

fn filter(item: &Json) -> Filter {
    let strings = |key: &str| match member(item, key) {
        Some(Json::Arr(values)) => values.iter().map(js_string).collect(),
        _ => Vec::new(),
    };
    Filter {
        label: member(item, "label").map(js_string).unwrap_or_default(),
        globs: strings("globs"),
        mimes: strings("mimes"),
    }
}

// JavaScript's String(value || ""): a falsy value is empty, anything else is its string form.
fn js_string(value: &Json) -> String {
    match value {
        Json::Null | Json::Bool(false) => String::new(),
        Json::Bool(true) => "true".into(),
        Json::Num(literal) if literal.parse::<f64>() == Ok(0.) => String::new(),
        Json::Num(literal) => literal.clone(),
        Json::Str(text) => text.clone(),
        Json::Arr(items) => items
            .iter()
            .map(element_string)
            .collect::<Vec<_>>()
            .join(","),
        Json::Obj(_) => "[object Object]".into(),
    }
}

// An array element keeps a falsy number or false as written, as Array.prototype.join does.
fn element_string(value: &Json) -> String {
    match value {
        Json::Null => String::new(),
        Json::Bool(false) => "false".into(),
        Json::Num(literal) => literal.clone(),
        other => js_string(other),
    }
}
