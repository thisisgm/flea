// What Enter, Space, h and l do to a Settings row, from ui/SettingsPanel.qml activate and stepRowValue with ui/ViewState.qml's writers.
use super::menus::{master_state, toggle_id, toggle_master};
use super::pages::{at, columns, hidden, is_true, shelf_enabled};
use super::rows::{focusable, Kind, Master, Row};
use super::textsize::{following, parse, pin, stepped, to_json, Stored};
use crate::jsondoc::Json;
use crate::keys::PRESETS;

#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    None,
    // A ui.json patch for the state writer to merge.
    Patch(Json),
    // Another page of the panel, such as Columns and back to View.
    Open(&'static str),
    // A row whose effect lives outside the panel: a favourite, the start folder, a web route, an update check.
    Host(String),
}

fn object(pairs: Vec<(&str, Json)>) -> Json {
    Json::Obj(pairs.into_iter().map(|(key, value)| (key.to_owned(), value)).collect())
}

fn strings(list: &[String]) -> Json {
    Json::Arr(list.iter().map(|id| Json::Str(id.clone())).collect())
}

// ViewState.changeSetting: one dotted step names a leaf, and the rail is remembered as a word.
pub fn setting(id: &str, value: Json) -> Json {
    if id == "places.rail" {
        let word = if value.as_bool() == Some(true) { "shown" } else { "hidden" };
        return object(vec![("places", object(vec![("rail", Json::Str(word.into()))]))]);
    }
    match id.split_once('.') {
        Some((group, leaf)) => object(vec![(group, object(vec![(leaf, value)]))]),
        None => object(vec![(id, value)]),
    }
}

// The Menus writer keeps the shelf out of the hidden set, since the shelf is one stored boolean of its own.
fn menu_hidden(list: Vec<String>) -> Json {
    let kept: Vec<String> = list.into_iter().filter(|id| id != "shelf").collect();
    object(vec![("menu", object(vec![("hidden", strings(&kept))]))])
}

// ViewState.setStartFolder: choosing a folder also selects the folder start mode, so both keys go in one write.
pub fn start_folder(path: &str) -> Json {
    object(vec![("startIn", Json::Str("folder".into())), ("startFolder", Json::Str(path.into()))])
}

fn merged(mut first: Json, second: Json) -> Json {
    if let (Json::Obj(into), Json::Obj(from)) = (&mut first, second) {
        into.extend(from);
    }
    first
}

fn text_size(stored: Stored) -> Json {
    object(vec![("display", object(vec![("textSize", to_json(&stored))]))])
}

fn stored_size(data: &Json) -> Stored {
    parse(at(data, &["display", "textSize"]).unwrap_or(&Json::Null))
}

// Enter and Space. A choice steps forward rather than opening a menu, since a walk reaches every value.
/// A shown column's grip dragged this many rows: it leaves its place and lands that many shown columns away, held
/// at either end, and the stored order is the order the list draws them in. A hidden column has no place to move to.
pub fn move_column(row: &Row, data: &Json, rows: isize) -> Edit {
    let Some(column) = row.id.strip_prefix("column:") else {
        return Edit::None;
    };
    let mut shown = columns(data);
    // The name is always shown and always first; only the metadata columns trade places.
    let mut movable: Vec<String> = shown.iter().filter(|key| *key != "name").cloned().collect();
    let Some(from) = movable.iter().position(|key| key == column) else {
        return Edit::None;
    };
    let to = (from as isize + rows).clamp(0, movable.len() as isize - 1) as usize;
    if to == from {
        return Edit::None;
    }
    let key = movable.remove(from);
    movable.insert(to, key);
    let mut rest = movable.into_iter();
    for slot in shown.iter_mut().filter(|key| *key != "name") {
        *slot = rest.next().unwrap_or_default();
    }
    Edit::Patch(object(vec![("columns", strings(&shown))]))
}

pub fn activate(section: &str, row: &Row, data: &Json, base: u32) -> Edit {
    if !focusable(row) {
        return Edit::None;
    }
    if row.kind == Kind::Favourite {
        return Edit::Host(row.id.clone());
    }
    if row.kind == Kind::Group {
        if !row.action.is_empty() {
            return Edit::Host(row.action.to_owned());
        }
        let stored = hidden(data);
        let patch = menu_hidden(toggle_master(&stored, &row.ids));
        if row.ids.contains(&"shelf") {
            let on = master_state(&stored, &row.ids) != Master::All;
            return Edit::Patch(merged(object(vec![("shelf", object(vec![("enabled", Json::Bool(on))]))]), patch));
        }
        return Edit::Patch(patch);
    }
    match row.id.as_str() {
        "columns" => return Edit::Open("columns"),
        "backView" => return Edit::Open("view"),
        "startFolder" | "keyboardSheet" | "updateFlea" | "makeDefault" | "language" => return Edit::Host(row.id.clone()),
        _ => {}
    }
    if !row.url.is_empty() {
        return Edit::Host(row.url.to_owned());
    }
    if let Some(column) = row.id.strip_prefix("column:") {
        let mut shown = columns(data);
        match shown.iter().position(|key| key == column) {
            Some(index) => {
                shown.remove(index);
            }
            None => shown.push(column.to_owned()),
        }
        return Edit::Patch(object(vec![("columns", strings(&shown))]));
    }
    if row.kind == Kind::Check && section != "menus" {
        return Edit::Patch(setting(&row.id, Json::Bool(!row.on)));
    }
    if row.id == "keyHints" {
        return Edit::Patch(object(vec![("keyHints", Json::Bool(!is_true(data, &["keyHints"])))]));
    }
    if row.kind == Kind::Check {
        if row.id == "shelf" {
            return Edit::Patch(object(vec![("shelf", object(vec![("enabled", Json::Bool(!shelf_enabled(data)))]))]));
        }
        return Edit::Patch(menu_hidden(toggle_id(&hidden(data), &row.id)));
    }
    step(row, data, base, 1)
}

// h, l and the chevrons: a choice walks its values with wrap, the text controls follow or step, anything else stays.
pub fn step(row: &Row, data: &Json, base: u32, direction: i32) -> Edit {
    if !focusable(row) {
        return Edit::None;
    }
    let stored = stored_size(data);
    match row.id.as_str() {
        "textMode" => return Edit::Patch(text_size(if following(stored) { pin(stored, base) } else { Stored::Follow })),
        "textStop" => return Edit::Patch(text_size(stepped(stored, base, direction))),
        _ => {}
    }
    if row.kind != Kind::Choice {
        return Edit::None;
    }
    let walk = |list: &[String], current: &str| {
        let index = list.iter().position(|value| value == current).map_or(-1, |index| index as i32);
        list[(index + direction).rem_euclid(list.len() as i32) as usize].clone()
    };
    if row.id == "preset" {
        let presets: Vec<String> = PRESETS.iter().map(|name| (*name).to_owned()).collect();
        let current = at(data, &["keys"]).and_then(Json::as_str).unwrap_or("default");
        return Edit::Patch(object(vec![("keys", Json::Str(walk(&presets, current)))]));
    }
    if row.values.is_empty() {
        return Edit::None;
    }
    let next = walk(&row.values, &row.selected);
    // The two numeric choices store numbers, as their schema rules require.
    let value = match row.id.as_str() {
        "columnsLimit" | "places.sidebarWidth" => Json::Num(next),
        _ => Json::Str(next),
    };
    Edit::Patch(setting(&row.id, value))
}

// A pointer on a segment cell or a ruler tick, addressed by index, from pickRowStop: one writer per row, as a key reaches.
pub fn pick(row: &Row, data: &Json, base: u32, stop: usize) -> Edit {
    if !focusable(row) {
        return Edit::None;
    }
    if row.id == "textMode" {
        return step(row, data, base, 1);
    }
    // The preset is a choice the oracle holds without values, so its cell names a preset outright.
    if row.id == "preset" {
        return PRESETS.get(stop).map_or(Edit::None, |name| Edit::Patch(object(vec![("keys", Json::Str((*name).to_owned()))])));
    }
    if let Some(value) = row.values.get(stop) {
        let value = match row.id.as_str() {
            "columnsLimit" | "places.sidebarWidth" => Json::Num(value.clone()),
            _ => Json::Str(value.clone()),
        };
        return Edit::Patch(setting(&row.id, value));
    }
    match row.stops.get(stop) {
        Some(size) if row.kind == Kind::Ruler => Edit::Patch(text_size(Stored::Stop(*size))),
        _ => Edit::None,
    }
}

#[cfg(test)]
#[path = "edit_tests.rs"]
mod tests;
