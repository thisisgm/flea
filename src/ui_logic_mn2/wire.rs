//! The `actions` wire family the backend answers (`src/backend/actions.rs`): `list`, `run` and `preview` request shapes, `actions`, `actionpreview` and `actionstarted` replies.
use crate::json::escape;
use crate::jsondoc::{self, Json};

fn quoted(items: &[String]) -> String {
    items
        .iter()
        .map(|p| format!("\"{}\"", escape(p)))
        .collect::<Vec<_>>()
        .join(",")
}

// Discovery runs off the loop for an explicit selection, so every request carries its own snapshot.
pub fn list_request(id: usize, paths: &[String]) -> String {
    format!(
        "{{\"c\":\"actions\",\"op\":\"list\",\"id\":{id},\"paths\":[{}]}}",
        quoted(paths)
    )
}

pub fn run_request(id: usize, action: &str, paths: &[String]) -> String {
    format!(
        "{{\"c\":\"actions\",\"op\":\"run\",\"id\":{id},\"action\":\"{}\",\"paths\":[{}]}}",
        escape(action),
        quoted(paths)
    )
}

// A preview names one cursor file; the backend refuses anything else.
pub fn preview_request(id: usize, action: &str, paths: &[String]) -> String {
    format!(
        "{{\"c\":\"actions\",\"op\":\"preview\",\"id\":{id},\"action\":\"{}\",\"paths\":[{}]}}",
        escape(action),
        quoted(paths)
    )
}

// One matched manifest or legacy script, in the backend's sort order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub selection: String,
    pub script: bool,
    // The MIME globs the action takes, as its manifest names them; a script takes "*/*".
    pub mime: Vec<String>,
}

// A manifest refusal the backend could not parse, kept with the file it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    pub file: String,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub id: usize,
    pub actions: Vec<Action>,
    pub errors: Vec<Discovery>,
    pub error: String,
}

fn text(doc: &Json, key: &str) -> String {
    doc.get(key)
        .and_then(Json::as_str)
        .unwrap_or_default()
        .into()
}

fn number(doc: &Json, key: &str) -> usize {
    doc.get(key).and_then(Json::as_f64).unwrap_or(0.0) as usize
}

// A malformed reply is a dropped discovery, never a row: the menu shows no Actions row for it.
// Sample input: {"t":"actions","id":4,"actions":[{"id":"zip","name":"Zip","mode":"background","selection":"many","script":false}],"errors":[{"file":"r.toml","error":"unknown action mode"}],"error":""}.
pub fn parse_listing(line: &str) -> Option<Listing> {
    let doc = jsondoc::parse(line).ok()?;
    if doc.get("t").and_then(Json::as_str) != Some("actions") {
        return None;
    }
    let action = |item: &Json| {
        Some(Action {
            id: item.get("id").and_then(Json::as_str)?.into(),
            name: item.get("name").and_then(Json::as_str)?.into(),
            mode: text(item, "mode"),
            selection: text(item, "selection"),
            script: item.get("script").and_then(Json::as_bool).unwrap_or(false),
            mime: match item.get("mime") {
                Some(Json::Arr(globs)) => globs.iter().filter_map(Json::as_str).map(String::from).collect(),
                _ => Vec::new(),
            },
        })
    };
    let actions = match doc.get("actions") {
        Some(Json::Arr(items)) => items.iter().filter_map(action).collect(),
        _ => return None,
    };
    let errors = match doc.get("errors") {
        Some(Json::Arr(items)) => items
            .iter()
            .map(|item| Discovery {
                file: text(item, "file"),
                error: text(item, "error"),
            })
            .collect(),
        _ => return None,
    };
    Some(Listing {
        id: number(&doc, "id"),
        actions,
        errors,
        error: text(&doc, "error"),
    })
}

// The preview's code-renderer text, or the fixed failure sentence it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    pub id: usize,
    pub ok: bool,
    pub text: String,
    pub error: String,
}

// Sample input: {"t":"actionpreview","id":5,"ok":true,"text":"zip -r out.zip notes.txt","error":""}.
pub fn parse_preview(line: &str) -> Option<Preview> {
    let doc = jsondoc::parse(line).ok()?;
    if doc.get("t").and_then(Json::as_str) != Some("actionpreview") {
        return None;
    }
    Some(Preview {
        id: number(&doc, "id"),
        ok: doc.get("ok").and_then(Json::as_bool).unwrap_or(false),
        text: text(&doc, "text"),
        error: text(&doc, "error"),
    })
}

// The run handshake: background modes land in Jobs, terminal modes open one terminal per path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Started {
    pub request: usize,
    pub id: usize,
    pub ok: bool,
    pub error: String,
}

// Sample input: {"t":"actionstarted","requestId":6,"id":40,"ok":true,"error":""}.
pub fn parse_started(line: &str) -> Option<Started> {
    let doc = jsondoc::parse(line).ok()?;
    if doc.get("t").and_then(Json::as_str) != Some("actionstarted") {
        return None;
    }
    Some(Started {
        request: number(&doc, "requestId"),
        id: number(&doc, "id"),
        ok: doc.get("ok").and_then(Json::as_bool).unwrap_or(false),
        error: text(&doc, "error"),
    })
}

// The backend's fixed refusal sentences, each said through its own catalog key.
pub const STALE_KEY: &str = "actions.error.stale";
pub const CURSOR_ONLY_KEY: &str = "actions.error.cursor_only";
pub const UNKNOWN_KEY: &str = "actions.error.unknown";
pub const PATHS_KEY: &str = "actions.error.paths";
pub const CANCELLED_KEY: &str = "actions.error.cancelled";
pub const EXITED_KEY: &str = "actions.error.exited";
pub const PREVIEW_CURSOR_KEY: &str = "actions.preview.cursor_only";
pub const PREVIEW_SANDBOX_KEY: &str = "actions.preview.no_sandbox";
pub const PREVIEW_SUPERSEDED_KEY: &str = "actions.preview.superseded";
pub const PREVIEW_FILE_KEY: &str = "actions.preview.regular_file";
pub const PREVIEW_PROGRAM_KEY: &str = "actions.preview.no_program";
pub const PREVIEW_OUTPUT_KEY: &str = "actions.preview.no_output";
pub const PREVIEW_FAILED_KEY: &str = "actions.preview.failed";
pub const PREVIEW_TIMEOUT_KEY: &str = "actions.preview.timed_out";
pub const PREVIEW_LARGE_KEY: &str = "actions.preview.too_large";

// Selections wider than this never discover: the backend refuses them, so the menu skips the request and stays quiet (mirrors `MAX_SELECTION` in `src/backend/actions.rs`).
pub const MAX_SELECTION: usize = 4096;

// The machine identity of a refusal: the key selects the sentence, the detail is its one argument.
pub fn refusal_key(error: &str) -> Option<&'static str> {
    Some(match error {
        "That action no longer matches the selection." => STALE_KEY,
        "Preview actions run only for the cursor row." => CURSOR_ONLY_KEY,
        "Unknown Actions request." => UNKNOWN_KEY,
        "Actions require absolute selection paths." => PATHS_KEY,
        "Action cancelled." => CANCELLED_KEY,
        "Preview actions require only the cursor row." => PREVIEW_CURSOR_KEY,
        "Preview sandbox unavailable." => PREVIEW_SANDBOX_KEY,
        "Preview superseded." => PREVIEW_SUPERSEDED_KEY,
        "Preview actions require a regular cursor file." => PREVIEW_FILE_KEY,
        "Preview program is unavailable." => PREVIEW_PROGRAM_KEY,
        "Preview output unavailable." => PREVIEW_OUTPUT_KEY,
        "Preview action failed." => PREVIEW_FAILED_KEY,
        "Preview output timed out." => PREVIEW_TIMEOUT_KEY,
        "Preview output exceeds 1048576 bytes." => PREVIEW_LARGE_KEY,
        _ if error.starts_with("Action exited with ") && error.ends_with('.') => EXITED_KEY,
        _ => return None,
    })
}

// The one argument of a keyed refusal: the exited status it names, so the sentence never carries an unfilled hole; every other refusal takes none.
pub fn refusal_args(error: &str) -> Vec<String> {
    match refusal_key(error) {
        Some(EXITED_KEY) => vec![error
            .strip_prefix("Action exited with ")
            .and_then(|rest| rest.strip_suffix('.'))
            .unwrap_or_default()
            .to_string()],
        _ => Vec::new(),
    }
}

// The catalog key for a manifest syntax fault: the file is %1, the backend's own diagnostic is %2.
pub const SYNTAX_KEY: &str = "actions.manifest.syntax";
// The catalog key for a manifest read fault: the file is %1, the OS reason is %2.
pub const READ_KEY: &str = "actions.manifest.io";

// A discovery refusal off the list reply: the key selects the sentence, the file is %1 and the named field, when the backend names one, is %2.
pub fn manifest_key(file: &str, error: &str) -> Option<(&'static str, Vec<String>)> {
    let filed = || vec![file.to_string()];
    let keyed = |key: &'static str| Some((key, filed()));
    let syntax = |diagnostic: &str| Some((SYNTAX_KEY, vec![file.to_string(), diagnostic.to_string()]));
    if let Some(field) = error
        .strip_suffix(" must be a nonempty string")
        .or_else(|| error.strip_suffix(" must be a nonempty string array"))
    {
        let key = if error.ends_with("array") {
            "actions.manifest.empty_list"
        } else {
            "actions.manifest.empty_text"
        };
        return Some((key, vec![file.to_string(), field.to_string()]));
    }
    match error {
        "too many manifests" => keyed("actions.manifest.too_many"),
        "not a regular readable manifest" => keyed("actions.manifest.unreadable"),
        "manifest exceeds 16384 bytes" => keyed("actions.manifest.too_large"),
        "manifest is not UTF-8" => keyed("actions.manifest.encoding"),
        "unknown manifest key" => keyed("actions.manifest.unknown_key"),
        "duplicate manifest key" => keyed("actions.manifest.duplicate_key"),
        "unexpected text after value" => keyed("actions.manifest.trailing"),
        "selection must be one or many" => keyed("actions.manifest.selection"),
        "unknown action mode" => keyed("actions.manifest.mode"),
        "MIME globs accept literal characters, * and ?" => keyed("actions.manifest.mime"),
        "the program must be literal" => keyed("actions.manifest.program"),
        "unclosed placeholder" => keyed("actions.manifest.unclosed"),
        "unknown placeholder" => keyed("actions.manifest.placeholder"),
        "too many array values" => syntax("too many array values"),
        "invalid string escape" => syntax("invalid string escape"),
        "control character in string" => syntax("control character in string"),
        "unclosed string" => syntax("unclosed string"),
        _ => match error.strip_prefix("expected ") {
            Some(want) => syntax(&format!("expected {want}")),
            // A read fault off `read_to_end` carries the OS reason, never a fixed sentence.
            None if error.contains("(os error ") => {
                Some((READ_KEY, vec![file.to_string(), error.to_string()]))
            }
            None => None,
        },
    }
}
