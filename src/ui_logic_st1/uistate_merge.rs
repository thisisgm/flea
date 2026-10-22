// The merges that fold a refused write or another window's bytes into a window's document (UiState.js).
use super::obj::{del, find, keys_of, put, same, truthy};
use super::{from_file, invalid_keys, is_group, is_whole_key, is_window_key, is_window_leaf, External};
use crate::jsondoc::{parse, Json};

// Drops only refused keys a retry can never land, keeping a valid key and a newer owed value.
pub fn drop_invalid(unsaved: &Json, inflight_text: &str) -> Json {
    let refused = invalid_keys(inflight_text);
    if refused.is_empty() {
        return unsaved.clone();
    }
    // Refused keys come only from a document that parsed, so this parse cannot fail.
    let inflight = parse(inflight_text).unwrap_or(Json::Null);
    let mut out = keys_of(unsaved);
    for key in refused {
        let Some(owed) = find(&out, key).cloned() else { continue };
        let carried = inflight.get(key);
        if is_group(Some(&owed)) && is_group(carried) {
            let mut kept = Vec::new();
            for (leaf, value) in keys_of(&owed) {
                if carried.and_then(|c| c.get(&leaf)).is_some_and(|c| same(Some(&value), Some(c))) {
                    continue;
                }
                kept.push((leaf, value));
            }
            if kept.is_empty() {
                del(&mut out, key);
            } else {
                put(&mut out, key, Json::Obj(kept));
            }
        } else if same(Some(&owed), carried) {
            del(&mut out, key);
        }
    }
    Json::Obj(out)
}

// Reverts only refused keys still showing the refused value, so a newer change behind that writer survives.
pub fn reverted_state(state: &Json, inflight_text: &str, settled: &Json) -> Json {
    let refused = invalid_keys(inflight_text);
    if refused.is_empty() {
        return state.clone();
    }
    let Ok(inflight @ Json::Obj(_)) = parse(inflight_text) else { return state.clone() };
    if !truthy(settled) {
        return state.clone();
    }
    let mut out = keys_of(state);
    for key in refused {
        let (Some(carried), Some(settled_value)) = (inflight.get(key), settled.get(key)) else { continue };
        let current = find(&out, key).cloned();
        if is_group(Some(carried)) && is_group(Some(settled_value)) && is_group(current.as_ref()) {
            let mut group = current.as_ref().map(keys_of).unwrap_or_default();
            for (leaf, carried_leaf) in keys_of(carried) {
                if !same(find(&group, &leaf), Some(&carried_leaf)) {
                    continue;
                }
                match settled_value.get(&leaf) {
                    Some(value) => put(&mut group, &leaf, value.clone()),
                    None => del(&mut group, &leaf),
                }
            }
            put(&mut out, key, Json::Obj(group));
        } else if same(current.as_ref(), Some(carried)) {
            put(&mut out, key, settled_value.clone());
        }
    }
    Json::Obj(out)
}

// A change another window saved: keys the file names and this window does not owe take the file's value.
pub fn apply_external(state: &Json, unsaved: &Json, text: &str) -> External {
    let read = from_file(text);
    if read.unreadable {
        return External { state: state.clone(), changed: false };
    }
    let mut out = keys_of(state);
    let mut changed = false;
    for (key, value) in keys_of(&read.state) {
        if is_window_key(&key) {
            continue;
        }
        if key == "places" && is_group(Some(&value)) {
            changed |= merge_places(&mut out, state, unsaved, &value);
            continue;
        }
        if let Some(owed) = unsaved.get(&key) {
            if let (Json::Obj(owed_leaves), Json::Obj(_), false) = (owed, &value, is_whole_key(&key)) {
                let mut group = keys_of(&value);
                let current = state.get(&key);
                for (leaf, _) in owed_leaves {
                    match current.and_then(|c| c.get(leaf)) {
                        Some(held) => put(&mut group, leaf, held.clone()),
                        None => del(&mut group, leaf),
                    }
                }
                let window_leaves: Vec<String> =
                    group.iter().filter(|(w, _)| is_window_leaf(&key, w)).map(|(w, _)| w.clone()).collect();
                for w in window_leaves {
                    match current.and_then(|c| c.get(&w)) {
                        Some(held) => put(&mut group, &w, held.clone()),
                        None => del(&mut group, &w),
                    }
                }
                let group = Json::Obj(group);
                if !same(Some(&group), state.get(&key)) {
                    put(&mut out, &key, group);
                    changed = true;
                }
            }
            continue;
        }
        if let (Json::Obj(file_leaves), Some(held_group)) =
            (&value, state.get(&key).filter(|h| matches!(h, Json::Obj(_))))
        {
            if file_leaves.iter().any(|(leaf, _)| is_window_leaf(&key, leaf)) {
                let mut leaf_group: Vec<(String, Json)> =
                    file_leaves.iter().filter(|(leaf, _)| !is_window_leaf(&key, leaf)).cloned().collect();
                for (leaf, held) in keys_of(held_group) {
                    if is_window_leaf(&key, &leaf) {
                        put(&mut leaf_group, &leaf, held);
                    }
                }
                let leaf_group = Json::Obj(leaf_group);
                if !same(Some(&leaf_group), state.get(&key)) {
                    put(&mut out, &key, leaf_group);
                    changed = true;
                }
                continue;
            }
        }
        if !same(Some(&value), state.get(&key)) {
            put(&mut out, &key, value);
            changed = true;
        }
    }
    if changed {
        External { state: Json::Obj(out), changed }
    } else {
        External { state: state.clone(), changed }
    }
}

// The places group from a file keeps this window's favourites, its owed leaves and its rail.
fn merge_places(out: &mut Vec<(String, Json)>, state: &Json, unsaved: &Json, file: &Json) -> bool {
    let held = state.get("places");
    let mut merged = Vec::new();
    for (field, value) in keys_of(file) {
        if field != "favourites" {
            put(&mut merged, &field, value);
        }
    }
    if let Some(favourites) = held.and_then(|h| h.get("favourites")) {
        put(&mut merged, "favourites", favourites.clone());
    }
    if let Some(Json::Obj(owed_leaves)) = unsaved.get("places") {
        for (leaf, _) in owed_leaves {
            if leaf == "favourites" {
                continue;
            }
            match held.and_then(|h| h.get(leaf)) {
                Some(value) => put(&mut merged, leaf, value.clone()),
                None => del(&mut merged, leaf),
            }
        }
    }
    if let Some(rail) = held.and_then(|h| h.get("rail")) {
        put(&mut merged, "rail", rail.clone());
    }
    let merged = Json::Obj(merged);
    if same(Some(&merged), state.get("places")) {
        return false;
    }
    put(out, "places", merged);
    true
}
