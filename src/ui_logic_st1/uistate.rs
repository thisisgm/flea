// ViewState.qml's settings bookkeeping from ui/js/UiState.js (Flea 0.3.12), over Json in JS key order, a JS undefined as None.
use crate::jsondoc::{parse, Json};
use crate::ui_logic_st1::Text;

#[path = "uistate_obj.rs"]
mod obj;
#[path = "uistate_merge.rs"]
mod merge;

use obj::{keys_of, put, same, truthy};

pub use merge::{apply_external, drop_invalid, reverted_state};

// Per-window keys: where a window is, not how Flea behaves (WINDOW_KEYS in UiState.js).
pub const WINDOW_KEYS: &[&str] = &[
    "view", "pickerView", "columnWidths", "dual", "lastPath", "lastTabs", "trashSweptOn", "stateVersion", "sort",
    "previewFacts",
];

// Per-window leaves, as "key.leaf" (WINDOW_LEAF_KEYS in UiState.js).
pub const WINDOW_LEAF_KEYS: &[&str] = &["places.rail", "preview.column", "preview.thumbSize"];

// The column names and the schema's shape for the columns key (invalidKeys in UiState.js).
const COLUMN_KEYS: &[&str] = &["name", "mode", "size", "date", "kind"];

// The document a window reads from ui.json, and whether it could not be read.
#[derive(Clone, Debug, PartialEq)]
pub struct FileRead {
    pub state: Json,
    pub unreadable: bool,
}

// One writer's bytes: saved on disk, in flight, and queued behind the running writer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Book {
    pub saved: String,
    pub inflight: String,
    pub pending: String,
}

// The next book, the patch to launch now (empty for none), and whether the writer was refused.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Step {
    pub book: Book,
    pub start: String,
    pub failed: bool,
}

// A settler's answer. Each half lands on its own, so either field may still be missing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Answer {
    pub code: Option<i64>,
    pub text: Option<String>,
}

// A prune request: queued behind a running settle, or started at once (empty for none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Prune {
    pub queue: String,
    pub start: String,
}

// The document after an external change, and whether any key moved.
#[derive(Clone, Debug, PartialEq)]
pub struct External {
    pub state: Json,
    pub changed: bool,
}

// The document after an external favourites read, and the error to show (empty for none).
#[derive(Clone, Debug, PartialEq)]
pub struct Refresh {
    pub state: Json,
    pub error: Text,
}

// One favourites edit this window made, as the record list sees it.
#[derive(Clone, Debug, PartialEq)]
pub enum FavouriteOp {
    Add(Json),
    Remove(usize),
    Move { index: usize, to: usize },
    Rename { index: usize, label: String },
}

// The answer a settler that never started is given: no real exit status is negative.
pub const NEVER_RAN: Answer = Answer { code: Some(-1), text: Some(String::new()) };

// The window's own read of ui.json: an unreadable document stays as written, and no file is a first launch.
pub fn from_file(text: &str) -> FileRead {
    match parse(text) {
        Ok(found @ Json::Obj(_)) => FileRead { state: found, unreadable: false },
        _ => FileRead { state: Json::Obj(Vec::new()), unreadable: !text.is_empty() },
    }
}

// External favourites update, independent of this window's drafts and pending patches.
pub fn refreshed_favourites(state: &Json, text: &str) -> Refresh {
    let read = from_file(text);
    let places = read.state.get("places");
    let places_ok = places.is_none() || matches!(places, Some(Json::Obj(_)));
    let favourites = places.and_then(|p| p.get("favourites"));
    let unreadable = text.is_empty()
        || read.unreadable
        || !places_ok
        || favourites.is_some_and(|f| !matches!(f, Json::Arr(_)));
    if unreadable {
        return Refresh { state: state.clone(), error: Text::Key("settings.error.favourites_unread") };
    }
    let records = Json::Arr(favourites.and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default());
    // The held list is (state.places || {}).favourites || [], so a falsy value reads as empty.
    let held = state.get("places").and_then(|p| p.get("favourites")).filter(|f| truthy(f));
    let held_list = held.cloned().unwrap_or_else(|| Json::Arr(Vec::new()));
    if same(Some(&held_list), Some(&records)) {
        return Refresh { state: state.clone(), error: Text::Empty };
    }
    let next = Json::Obj(vec![("favourites".to_string(), records)]);
    Refresh { state: with_group(state, "places", &next), error: Text::Empty }
}

// Compare a save with this window's intent, since the writer response can carry concurrent edits.
pub fn favourites_after(records: &[Json], op: &FavouriteOp) -> Vec<Json> {
    let mut next = records.to_vec();
    match op {
        FavouriteOp::Add(record) => next.push(record.clone()),
        FavouriteOp::Remove(index) => {
            if *index < next.len() {
                next.remove(*index);
            }
        }
        FavouriteOp::Move { index, to } => {
            // A missing source is JS undefined, and it lands in the list as null.
            let moved = if *index < next.len() { next.remove(*index) } else { Json::Null };
            let at = (*to).min(next.len());
            next.insert(at, moved);
        }
        FavouriteOp::Rename { index, label } => {
            if next.len() <= *index {
                next.resize(index + 1, Json::Null);
            }
            let mut pairs = keys_of(&next[*index]);
            put(&mut pairs, "label", Json::Str(label.clone()));
            next[*index] = Json::Obj(pairs);
        }
    }
    next
}

pub fn is_window_key(key: &str) -> bool {
    WINDOW_KEYS.contains(&key)
}

pub fn is_window_leaf(key: &str, leaf: &str) -> bool {
    let joined = format!("{key}.{leaf}");
    WINDOW_LEAF_KEYS.contains(&joined.as_str())
}

// A patch with a refused key is never retried as is, but a transient failure is.
pub fn is_patch_invalid(patch_text: &str) -> bool {
    !invalid_keys(patch_text).is_empty()
}

// The top-level keys a schema would refuse, so only those prune and a valid key beside them never does.
pub fn invalid_keys(patch_text: &str) -> Vec<&'static str> {
    let mut refused = Vec::new();
    let Ok(patch) = parse(patch_text) else { return refused };
    if !matches!(patch, Json::Obj(_)) {
        return refused;
    }
    if patch.get("columns").is_some_and(columns_refused) {
        refused.push("columns");
    }
    if patch.get("places").is_some_and(|p| !matches!(p, Json::Obj(_))) {
        refused.push("places");
    }
    refused
}

// The columns list is an array holding "name" and only known names; a repeated name passes, as UiState.js lets it.
fn columns_refused(cols: &Json) -> bool {
    let Some(items) = cols.as_array() else { return true };
    if !items.iter().any(|c| c.as_str() == Some("name")) {
        return true;
    }
    items.iter().any(|c| !c.as_str().is_some_and(|s| COLUMN_KEYS.contains(&s)))
}

// True when text parses as a JSON object; empty or garbage is ignored until the next valid write.
pub fn parses_as_object(text: &str) -> bool {
    matches!(parse(text), Ok(Json::Obj(_)))
}

// Merges a half-collected answer into the whole one, whichever arrives first.
pub fn landed(answer: Answer, half: Answer) -> Answer {
    Answer { code: half.code.or(answer.code), text: half.text.or(answer.text) }
}

// An empty text is an answer too, so only a missing half counts as not whole.
pub fn whole(answer: &Answer) -> bool {
    answer.code.is_some() && answer.text.is_some()
}

// A settler that cannot start reports no exit code while running has gone false.
pub fn never_ran(answer: &Answer, running: bool) -> bool {
    !running && answer.code.is_none()
}

// A refused prune queues behind a running settle instead of taking over its mode.
pub fn prune_ask(running: bool, failed_patch: &str) -> Prune {
    if running {
        Prune { queue: failed_patch.to_string(), start: String::new() }
    } else {
        Prune { queue: String::new(), start: failed_patch.to_string() }
    }
}

// A settle is busy while its writer runs, or while its mode is set and an answer half is missing.
pub fn settle_busy(running: bool, mode: &str, answer: &Answer) -> bool {
    running || (!mode.is_empty() && !whole(answer))
}

// The settle's next step: a queued prune first, then a dirty re-read, else idle.
pub fn settle_next(dirty: bool, prune_queued: &str) -> &'static str {
    if !prune_queued.is_empty() {
        "prune"
    } else if dirty {
        "apply"
    } else {
        "idle"
    }
}

// Copies the document with one top-level key replaced. Writers rebuild rather than mutate, because QML notifies on assignment only.
pub fn with_key(state: &Json, key: &str, value: Json) -> Json {
    let mut out = keys_of(state);
    put(&mut out, key, value);
    Json::Obj(out)
}

// Merges next into the group at key, so a half written here keeps the other half.
pub fn with_group(state: &Json, key: &str, next: &Json) -> Json {
    let mut group = state.get(key).map(keys_of).unwrap_or_default();
    for (n, v) in keys_of(next) {
        put(&mut group, &n, v);
    }
    with_key(state, key, Json::Obj(group))
}

// A window's starting book: nothing of its own saved, and no writer running.
pub fn book() -> Book {
    Book::default()
}

// A change asks for a write. A patch equal to the newest one already sent starts nothing.
pub fn asked(b: &Book, patch: &str) -> Step {
    let newest = if !b.pending.is_empty() {
        &b.pending
    } else if !b.inflight.is_empty() {
        &b.inflight
    } else {
        &b.saved
    };
    if patch == newest {
        return Step { book: b.clone(), start: String::new(), failed: false };
    }
    if !b.inflight.is_empty() {
        let book = Book { saved: b.saved.clone(), inflight: b.inflight.clone(), pending: patch.to_string() };
        return Step { book, start: String::new(), failed: false };
    }
    let book = Book { saved: b.saved.clone(), inflight: patch.to_string(), pending: String::new() };
    Step { book, start: patch.to_string(), failed: false }
}

// The writer exited. Only a zero status proves the patch reached the file, and `owed` is what the window owes now.
pub fn exited(b: &Book, code: i64, owed: &str) -> Step {
    let next = if !b.pending.is_empty() && owed != "{}" { owed.to_string() } else { String::new() };
    let saved = if code == 0 { b.inflight.clone() } else { b.saved.clone() };
    Step {
        book: Book { saved, inflight: next.clone(), pending: String::new() },
        start: next,
        failed: code != 0,
    }
}

// What is still owed after a landed patch is taken out. A value changed while the writer ran stays owed.
pub fn acknowledged(unsaved: &Json, patch: &str) -> Json {
    let Ok(written @ Json::Obj(_)) = parse(patch) else { return unsaved.clone() };
    let mut out = Vec::new();
    for (key, owed) in keys_of(unsaved) {
        if let Some(still) = still_owed(&owed, written.get(&key), &key) {
            put(&mut out, &key, still);
        }
    }
    Json::Obj(out)
}

// One key of the owed patch against the same key of the landed one. None when the writer took it all.
pub fn still_owed(owed: &Json, landed: Option<&Json>, key: &str) -> Option<Json> {
    let Some(landed) = landed else { return Some(owed.clone()) };
    if is_whole_key(key) && is_group(Some(owed)) && is_group(Some(landed)) {
        return if same(Some(owed), Some(landed)) { None } else { Some(owed.clone()) };
    }
    if is_group(Some(owed)) && is_group(Some(landed)) {
        let mut kept = Vec::new();
        for (leaf, value) in keys_of(owed) {
            if same(Some(&value), landed.get(&leaf)) {
                continue;
            }
            kept.push((leaf, value));
        }
        return if kept.is_empty() { None } else { Some(Json::Obj(kept)) };
    }
    if same(Some(owed), Some(landed)) {
        None
    } else {
        Some(owed.clone())
    }
}

// A settings group: the only object shape withGroup builds. An array is a whole key's value.
pub fn is_group(value: Option<&Json>) -> bool {
    matches!(value, Some(Json::Obj(_)))
}

// Whole-value object keys are owed and cleared whole. Only lastTabs is one today.
pub fn is_whole_key(key: &str) -> bool {
    key == "lastTabs"
}

#[cfg(test)]
#[path = "uistate_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "uistate_tests_b.rs"]
mod tests_b;

#[cfg(test)]
#[path = "uistate_tests_c.rs"]
mod tests_c;

#[cfg(test)]
#[path = "uistate_tests_d.rs"]
mod tests_d;
