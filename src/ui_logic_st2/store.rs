//! Where a plugin's switch and settings live in ui.json, read and patched; src/uiplugins.rs keeps the map's shape.
use super::builtin::SHELF;
use super::schema::Value;
use crate::jsondoc::Json;

pub const KEY: &str = "plugins";

fn entry<'a>(state: &'a Json, id: &str) -> Option<&'a Json> {
    state.get(KEY).and_then(|map| map.get(id))
}

// The Shelf's one switch is shelf.enabled, which its bar plugin already reads; every other plugin keeps its own.
pub fn enabled(state: &Json, id: &str, shipped_on: bool) -> bool {
    if id == SHELF {
        return state.get("shelf").and_then(|s| s.get("enabled")).and_then(Json::as_bool) == Some(true);
    }
    entry(state, id).and_then(|e| e.get("enabled")).and_then(Json::as_bool).unwrap_or(shipped_on)
}

pub fn stored_settings<'a>(state: &'a Json, id: &str) -> Option<&'a Json> {
    entry(state, id).and_then(|e| e.get("settings"))
}

fn obj(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

pub fn enable_patch(id: &str, on: bool) -> Json {
    if id == SHELF {
        return obj(vec![("shelf", obj(vec![("enabled", Json::Bool(on))]))]);
    }
    obj(vec![(KEY, obj(vec![(id, obj(vec![("enabled", Json::Bool(on))]))]))])
}

// The Shelf's rows are the shelf group's own keys, which ui.json's schema already checks.
pub fn setting_patch(id: &str, key: &str, value: &Value) -> Json {
    if id == SHELF {
        return obj(vec![("shelf", obj(vec![(key, value.to_json())]))]);
    }
    obj(vec![(KEY, obj(vec![(id, obj(vec![("settings", obj(vec![(key, value.to_json())]))]))]))])
}

// Remove takes the plugin's settings with it, as `omarchy plugin remove` does.
pub fn forget_patch(id: &str) -> Json {
    obj(vec![(KEY, obj(vec![(id, Json::Null)]))])
}
