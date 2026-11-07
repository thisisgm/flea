// The one-time changes a stored ui.json is owed, applied on read and recorded by its stateVersion and menuVersion stamps.
use crate::jsondoc::Json;
use crate::uischema::{MENU_VERSION, STATE_VERSION};

// Each change by the stateVersion it arrived in, oldest first; a file stamped below one has not had it.
type Step = fn(Json) -> Json;
const STEPS: &[(f64, Step)] = &[
    (1.0, show_unmounted_drives),
    (2.0, hide_new_menu_rows),
    (3.0, hide_run_rows),
    (4.0, hide_0_4_0_rows),
];

// The menuVersion 0.4.1 leaves once its menu rows were swapped; DEFAULTS in src/uischema.rs carries the same.
const MENU_0_4_1: f64 = 1.0;

// What an untouched Settings > Menus list holds after steps 2 to 4: shipped by 0.4.0, by 0.3.6 to 0.3.12, by 0.3.0 to 0.3.5.
const UNTOUCHED_0_4_0: &[&[&str]] = &[
    &["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "properties", "permissions", "copyAs",
      "pasteAs", "invertSelection", "extThumbs", "run", "openAsText", "openAgent"],
    &["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "properties", "permissions", "copyAs",
      "pasteAs", "invertSelection", "extThumbs", "run", "openAsText", "openAgent", "actions"],
    &["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "properties", "permissions", "copyAs",
      "pasteAs", "invertSelection", "run", "openAsText", "openAgent", "actions"],
];

// Automatic settlement must leave a newer schema for the version that understands it.
pub fn newer_than_known(found: &Json) -> bool {
    STEPS.last().is_some_and(|(known, _)| stored_version(found) > *known)
}

// The file as found carries the stamp: the merged document already holds the shipped one in its place.
pub fn migrated(found: &Json, merged: Json) -> Json {
    let stored = stored_version(found);
    // Only a stored list the merge kept is a saved list; one that fell back is the shipped default.
    let hidden = |doc: &Json| doc.get("menu").and_then(|menu| menu.get("hidden")).cloned();
    let stored_menu = hidden(found).is_some() && hidden(found) == hidden(&merged);
    let mut state = merged;
    for (version, step) in STEPS {
        if stored < *version {
            // Menu migrations apply to a saved list, never the current unset default.
            let next = if *version >= 2.0 && !stored_menu { state } else { step(state) };
            state = with_key(next, STATE_VERSION, Json::Num(format!("{}", version)));
        }
    }
    // 0.4.0 keeps an unknown key as it was read, so this stamp survives a downgrade and the upgrade back.
    if found.get(MENU_VERSION).and_then(Json::as_f64).unwrap_or(0.0) < MENU_0_4_1 {
        if stored_menu {
            state = swap_0_4_1_rows(state);
        }
        state = with_key(state, MENU_VERSION, Json::Num(format!("{}", MENU_0_4_1)));
    }
    state
}

// Sample input: 0.3.2's {"places":{"showUnmounted":false},...} has no stamp and reads as 0; 0.3.3 writes {...,"stateVersion":1}.
fn stored_version(found: &Json) -> f64 {
    found.get(STATE_VERSION).and_then(Json::as_f64).unwrap_or(0.0)
}

// Decided in 0.3.3: turned on once for every file written before it, including one switched off in 0.3.2.
fn show_unmounted_drives(state: Json) -> Json {
    let places = state.get("places").cloned().unwrap_or(Json::Obj(Vec::new()));
    with_key(state, "places", with_key(places, "showUnmounted", Json::Bool(true)))
}

// A 0.3.7 file that hid "copypath" keeps hiding its replacement "copyAs" where it stood.
fn hide_new_menu_rows(state: Json) -> Json {
    let menu = state.get("menu").cloned().unwrap_or(Json::Obj(Vec::new()));
    let items = match menu.get("hidden").and_then(Json::as_array) {
        Some(items) => items.to_vec(),
        None => return state,
    };
    if items.iter().any(|item| !matches!(item, Json::Str(_))) {
        return state;
    }
    // copypath becomes copyAs where it stands; a file that held both keeps the first.
    let mut kept: Vec<String> = Vec::with_capacity(items.len() + 2);
    for item in items.iter().filter_map(Json::as_str) {
        let name = if item == "copypath" { "copyAs" } else { item };
        if name == "copyAs" && kept.iter().any(|held| held == "copyAs") {
            continue;
        }
        kept.push(name.to_string());
    }
    for owned in ["pasteAs", "invertSelection"] {
        if !kept.iter().any(|name| name == owned) {
            kept.push(owned.to_string());
        }
    }
    let hidden = Json::Arr(kept.into_iter().map(Json::Str).collect());
    with_key(state, "menu", with_key(menu, "hidden", hidden))
}

// The run card's own rows ship hidden, so a 0.3.8 file that never named them hides them once.
fn hide_run_rows(state: Json) -> Json {
    let menu = state.get("menu").cloned().unwrap_or(Json::Obj(Vec::new()));
    let items = match menu.get("hidden").and_then(Json::as_array) {
        Some(items) => items.to_vec(),
        None => return state,
    };
    if items.iter().any(|item| !matches!(item, Json::Str(_))) {
        return state;
    }
    let mut kept: Vec<String> = items.iter().filter_map(Json::as_str).map(str::to_string).collect();
    for owned in ["run", "openAsText"] {
        if !kept.iter().any(|name| name == owned) {
            kept.push(owned.to_string());
        }
    }
    let hidden = Json::Arr(kept.into_iter().map(Json::Str).collect());
    with_key(state, "menu", with_key(menu, "hidden", hidden))
}

// Open in Agent ships off, so a stored list that never named it hides it once; Actions ships with Run Script's place hidden, so only a list that hid Run Script hides Actions too, and nothing beyond those ids is removed, reordered or changed.
fn hide_0_4_0_rows(state: Json) -> Json {
    let menu = state.get("menu").cloned().unwrap_or(Json::Obj(Vec::new()));
    let items = match menu.get("hidden").and_then(Json::as_array) {
        Some(items) => items.to_vec(),
        None => return state,
    };
    if items.iter().any(|item| !matches!(item, Json::Str(_))) {
        return state;
    }
    let mut kept: Vec<String> = items.iter().filter_map(Json::as_str).map(str::to_string).collect();
    let mut owned = vec!["openAgent"];
    if kept.iter().any(|name| name == "runScript") {
        owned.push("actions");
    }
    for name in owned {
        if !kept.iter().any(|held| held == name) {
            kept.push(name.to_string());
        }
    }
    let hidden = Json::Arr(kept.into_iter().map(Json::Str).collect());
    with_key(state, "menu", with_key(menu, "hidden", hidden))
}

// 0.4.1 shows Properties and Permissions and takes the two view rows off the file menu, once, on a list equal to one an untouched file holds; any other list is kept exactly as saved.
fn swap_0_4_1_rows(state: Json) -> Json {
    let menu = state.get("menu").cloned().unwrap_or(Json::Obj(Vec::new()));
    let items = match menu.get("hidden").and_then(Json::as_array) {
        Some(items) => items.to_vec(),
        None => return state,
    };
    if items.iter().any(|item| !matches!(item, Json::Str(_))) {
        return state;
    }
    let mut held: Vec<&str> = items.iter().filter_map(Json::as_str).collect();
    held.sort_unstable();
    let untouched = UNTOUCHED_0_4_0.iter().any(|list| {
        let mut shipped = list.to_vec();
        shipped.sort_unstable();
        shipped == held
    });
    if !untouched {
        return state;
    }
    let mut kept: Vec<Json> = items
        .into_iter()
        .filter(|item| !matches!(item.as_str(), Some("properties" | "permissions")))
        .collect();
    kept.extend(["toggleHidden", "toggleLangMarks"].map(|id| Json::Str(id.into())));
    with_key(state, "menu", with_key(menu, "hidden", Json::Arr(kept)))
}

// Replaced where it stands, so the key order a rewrite renders is the merge's and not this file's.
fn with_key(object: Json, key: &str, value: Json) -> Json {
    let Json::Obj(mut pairs) = object else { return object };
    match pairs.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = value,
        None => pairs.push((key.to_string(), value)),
    }
    Json::Obj(pairs)
}

#[cfg(test)]
#[path = "uimigrate_tests.rs"]
mod tests;
