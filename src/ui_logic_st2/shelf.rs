//! The Shelf plugin's page, which holds every row v0.3.12's Shelf pane held (ui/js/SettingsShelf.js, ui/js/ShelfPile.js).
use super::pages::Row;
use super::schema::Value;
use crate::i18n::I18n;
use crate::jsondoc::{self, Json};

// The four counts the board offers; a hand-edited count off them reads as three.
pub const CAPTURE_COUNTS: [i64; 4] = [1, 2, 3, 5];
pub const EDGES: [&str; 4] = ["off", "left", "right", "bottom"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pin {
    pub path: String,
    pub name: String,
    pub folder: bool,
    pub missing: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub enabled: bool,
    pub bar: bool,
    pub rail: String,
    pub screenshots: bool,
    pub recordings: bool,
    pub recent: i64,
    pub pins: Vec<Pin>,
}

pub fn capture_count(value: Option<&Json>) -> i64 {
    match value.and_then(Json::as_f64) {
        Some(n) if n.fract() == 0.0 && CAPTURE_COUNTS.contains(&(n as i64)) => n as i64,
        _ => 3,
    }
}

pub fn rail_edge(value: Option<&Json>) -> String {
    match value.and_then(Json::as_str) {
        Some(edge) if EDGES.contains(&edge) => edge.into(),
        _ => "off".into(),
    }
}

// The ui.json shelf group, with the pile the bar plugin writes; the master ships off and the rows under it open on.
pub fn input(state: &Json, pins: Vec<Pin>) -> Input {
    let shelf = state.get("shelf");
    let flag = |key: &str| shelf.and_then(|s| s.get(key)).and_then(Json::as_bool);
    Input {
        enabled: flag("enabled") == Some(true),
        bar: flag("bar") != Some(false),
        rail: rail_edge(shelf.and_then(|s| s.get("rail"))),
        screenshots: flag("screenshots") != Some(false),
        recordings: flag("recordings") != Some(false),
        recent: capture_count(shelf.and_then(|s| s.get("recent"))),
        pins,
    }
}

fn leaf(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
}

// Sample input: {"items":[{"path":"/home/gm/Invoices","folder":true,"pinned":true}]}. A pile that cannot be read is no pins.
pub fn pinned(text: &str) -> Vec<Pin> {
    let doc = jsondoc::parse(text).unwrap_or(Json::Null);
    let items = doc.get("items").and_then(Json::as_array).unwrap_or(&[]);
    items
        .iter()
        .filter(|item| item.get("pinned").and_then(Json::as_bool) == Some(true))
        .filter_map(|item| {
            let path = item.get("path").and_then(Json::as_str)?;
            Some(Pin {
                path: path.into(),
                name: leaf(path),
                folder: item.get("folder").and_then(Json::as_bool) == Some(true),
                missing: item.get("missing").and_then(Json::as_bool) == Some(true),
            })
        })
        .collect()
}

// A Shelf row's write, checked the way its row offers it before ui.json's own schema sees it.
pub fn value(key: &str, raw: &Json) -> Option<Value> {
    match key {
        "bar" | "screenshots" | "recordings" => raw.as_bool().map(Value::Bool),
        "rail" => raw.as_str().filter(|e| EDGES.contains(e)).map(|e| Value::Choice(e.into())),
        "recent" => raw.as_f64().filter(|n| n.fract() == 0.0 && CAPTURE_COUNTS.contains(&(*n as i64))).map(|n| Value::Int(n as i64)),
        _ => None,
    }
}

fn choice(id: &str, label: String, glyph: &str, values: Vec<String>, labels: Vec<String>, selected: String, available: bool) -> Row {
    Row::Choice { id: id.into(), label, glyph: glyph.into(), values, labels, selected, available }
}

fn check(id: &str, label: String, glyph: &str, on: bool, available: bool) -> Row {
    Row::Check { id: id.into(), label, glyph: glyph.into(), on, available }
}

pub fn rows(input: &Input, i18n: &I18n) -> Vec<Row> {
    let on = input.enabled;
    let mut out = vec![Row::Group { id: "shelf.enabled".into(), label: i18n.tr("plugins.builtin.shelf.name"), switch: Some(on), action: None }];
    out.push(check("shelf.bar", i18n.tr("plugins.shelf.bar"), "shelf", input.bar, on));
    let edge_labels = ["off", "left", "right", "bottom"].iter().map(|e| i18n.tr(&format!("plugins.shelf.rail.{e}"))).collect();
    out.push(choice("shelf.rail", i18n.tr("plugins.shelf.rail"), "maximize", EDGES.iter().map(|e| e.to_string()).collect(), edge_labels, input.rail.clone(), on));
    // The bind is the user's line in Hyprland's config, and nothing here installs it.
    out.push(Row::Hint { text: i18n.tr("plugins.shelf.hint") });
    out.push(Row::Group { id: "shelf.captures".into(), label: i18n.tr("plugins.shelf.captures"), switch: None, action: None });
    out.push(check("shelf.screenshots", i18n.tr("plugins.shelf.screenshots"), "image", input.screenshots, on));
    out.push(check("shelf.recordings", i18n.tr("plugins.shelf.recordings"), "film", input.recordings, on));
    let counts: Vec<String> = CAPTURE_COUNTS.iter().map(|n| n.to_string()).collect();
    let count_labels = CAPTURE_COUNTS.iter().map(|n| i18n.locale().format_count(*n)).collect();
    out.push(choice("shelf.recent", i18n.tr("plugins.shelf.show"), "list", counts, count_labels, input.recent.to_string(), on));
    out.push(Row::Group { id: "pinFolder".into(), label: i18n.tr("plugins.shelf.pinned"), switch: None, action: Some(i18n.tr("plugins.shelf.pin_folder")) });
    for (index, pin) in input.pins.iter().enumerate() {
        out.push(Row::Favourite {
            id: format!("pin:{index}"), label: pin.name.clone(), path: pin.path.clone(),
            glyph: if pin.folder { "folder" } else { "file" }.into(),
            error: if pin.missing { i18n.tr("plugins.shelf.missing") } else { String::new() },
            available: on,
        });
    }
    out
}
