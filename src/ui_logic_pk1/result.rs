// The checks child's answer to one picker check, read from its own object so nested fields cannot impersonate it.
use crate::jsondoc::{self, Json};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckResult {
    pub id: u64,
    pub op: String,
    pub ok: bool,
    // The backend's own refusal; the producer owns its wording and the front end shows it as given.
    pub error: String,
    pub marks: Vec<(String, u64)>,
    pub removed: u64,
    pub skipped: usize,
    pub skipped_items: Vec<(String, String)>,
    pub review: u64,
    pub path: String,
    pub collision: bool,
}

// Sample input: {"t":"picker","id":3,"op":"mark","ok":true,"marks":[{"path":"/a","bytes":2}],"removed":0}
pub fn check_result(line: &str) -> Option<CheckResult> {
    let document = jsondoc::parse(line).ok()?;
    if document.get("t").and_then(Json::as_str) != Some("picker") {
        return None;
    }
    let whole = |key: &str| {
        document
            .get(key)
            .and_then(Json::as_f64)
            .filter(|value| *value >= 0. && value.fract() == 0. && *value < 9.0e15)
            .map(|value| value as u64)
    };
    let text = |key: &str| {
        document
            .get(key)
            .and_then(Json::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let marks = match document.get("marks") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|item| {
                let path = item.get("path").and_then(Json::as_str)?;
                let bytes = item
                    .get("bytes")
                    .and_then(Json::as_f64)
                    .filter(|bytes| *bytes >= 0.)?;
                Some((path.to_owned(), bytes as u64))
            })
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
        None => Vec::new(),
    };
    Some(CheckResult {
        id: whole("id").filter(|id| *id > 0)?,
        op: text("op"),
        ok: document.get("ok").and_then(Json::as_bool) == Some(true),
        error: text("error"),
        marks,
        removed: whole("removed").unwrap_or(0),
        skipped: document
            .get("skipped")
            .and_then(Json::as_array)
            .map_or(0, <[Json]>::len),
        skipped_items: document
            .get("skipped")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|item| {
                Some((
                    item.get("path")?.as_str()?.to_owned(),
                    item.get("why")?.as_str()?.to_owned(),
                ))
            })
            .collect(),
        review: whole("review").unwrap_or(0),
        path: text("path"),
        collision: document.get("collision").and_then(Json::as_bool) == Some(true),
    })
}
