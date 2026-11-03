// The Settings sections and the rows each pane draws, from ui/js/Settings.js; the panel only paints what these return.
use super::rows::{Kind, Row};
use super::Text;
use crate::jsondoc::Json;

#[path = "pages_display.rs"]
mod display;
#[path = "pages_places.rs"]
mod places;
#[path = "pages_view.rs"]
mod view;

pub use display::{display_rows, key_preview, key_rows, scale_label, user_binding_refusals, user_key_refusals, user_key_rows, UserKeyFacts, USER_KEYS};
pub use places::{favourite_entries, places_rows, sidebar_width, Favourite, WIDTH_STOPS};
pub use view::{preview_rows, view_rows};

pub struct Section {
    pub id: &'static str,
    pub label: &'static str,
    pub glyph: &'static str,
}

// The Shelf section reads Plugins in 0.4.0, under the plug mark, because the Shelf becomes a plugin.
pub const SECTIONS: &[Section] = &[
    Section { id: "view", label: "settings.section.view", glyph: "sliders" },
    Section { id: "places", label: "settings.section.places", glyph: "star" },
    Section { id: "plugins", label: "settings.section.plugins", glyph: "plug" },
    Section { id: "preview", label: "settings.section.preview", glyph: "preview" },
    Section { id: "keys", label: "settings.section.keys", glyph: "keyboard" },
    Section { id: "display", label: "settings.section.display", glyph: "maximize" },
    Section { id: "menus", label: "settings.section.menus", glyph: "list" },
    Section { id: "about", label: "settings.section.about", glyph: "info" },
];

// Where a section id sits in SECTIONS, or 0 for an id no section carries.
pub fn section_index(id: &str) -> usize {
    SECTIONS.iter().position(|section| section.id == id).unwrap_or(0)
}

// What the panel passes in: ui.json as read, and the facts Flea reads but never writes.
pub struct State<'a> {
    pub data: &'a Json,
    pub home: &'a str,
    pub preset: &'a str,
    pub base_size: u32,
    pub monitor_scale: f64,
    // None reads Saved; a failed write names itself and takes the error role.
    pub save_error: Option<Text>,
    pub favourite_statuses: &'a [Text],
    // The user's key file and the enabled plugins' menu actions it may name.
    pub user_keys: Option<&'a UserKeyFacts>,
}

impl<'a> State<'a> {
    pub fn new(data: &'a Json) -> Self {
        Self { data, home: "", preset: "default", base_size: 14, monitor_scale: 1., save_error: None, favourite_statuses: &[], user_keys: None }
    }
}

// Sample input: at(data, &["preview", "loadOn"]) is data.preview.loadOn, or None along any missing step.
pub fn at<'a>(data: &'a Json, path: &[&str]) -> Option<&'a Json> {
    path.iter().try_fold(data, |value, key| value.get(key))
}

// JS `=== true`: only a stored boolean true counts.
pub fn is_true(data: &Json, path: &[&str]) -> bool {
    at(data, path).and_then(Json::as_bool) == Some(true)
}

// JS `!== false`: anything but a stored boolean false counts, so a junk value heals to on.
pub fn not_false(data: &Json, path: &[&str]) -> bool {
    at(data, path).and_then(Json::as_bool) != Some(false)
}

// JS `value || fallback` for a word: an absent or empty string reads the fallback.
pub fn word<'a>(data: &'a Json, path: &[&str], fallback: &'a str) -> &'a str {
    at(data, path).and_then(Json::as_str).filter(|value| !value.is_empty()).unwrap_or(fallback)
}

// JS `value || fallback` for a number held as a JSON number; anything else reads the fallback.
pub fn number(data: &Json, path: &[&str], fallback: f64) -> f64 {
    at(data, path).and_then(|value| match value {
        Json::Num(_) => value.as_f64(),
        _ => None,
    }).filter(|value| *value != 0.).unwrap_or(fallback)
}

// The section's rows; Plugins pages are the plugin unit's, so the caller hands them in.
pub fn rows(section: &str, state: &State<'_>, plugins: Vec<Row>, about: impl FnOnce() -> Vec<Row>) -> Vec<Row> {
    match section {
        "columns" => column_rows(state.data),
        "places" => places_rows(state),
        "plugins" | "shelf" => plugins,
        "view" => view_rows(state),
        "preview" => preview_rows(state),
        "about" => about(),
        "display" => display_rows(state),
        "menus" => super::menus::rows(&hidden(state.data), is_true(state.data, &["keyHints"])),
        _ => key_rows(state),
    }
}

// ViewState.menuHidden: the stored ids or the shipped list, the shelf counted off unless shelf.enabled is true.
pub fn hidden(data: &Json) -> Vec<String> {
    let mut ids: Vec<String> = match at(data, &["menu", "hidden"]).and_then(Json::as_array) {
        Some(list) => list.iter().filter_map(Json::as_str).map(str::to_owned).collect(),
        None => super::menus::SHIPPED_HIDDEN.iter().map(|id| (*id).to_owned()).collect(),
    };
    ids.retain(|id| id != "shelf");
    if !shelf_enabled(data) {
        ids.push("shelf".to_owned());
    }
    ids
}

// ViewState.shelfEnabled is === true, so an absent or malformed switch is off.
pub fn shelf_enabled(data: &Json) -> bool {
    at(data, &["shelf", "enabled"]).and_then(Json::as_bool) == Some(true)
}

// The stored columns, or the shipped three when none are stored.
pub fn columns(data: &Json) -> Vec<String> {
    match at(data, &["columns"]).and_then(Json::as_array) {
        Some(list) => list.iter().filter_map(Json::as_str).map(str::to_owned).collect(),
        None => vec!["name".into(), "size".into(), "date".into()],
    }
}

// The list header's own name for each column, so Columns and Sort by both say Modified.
pub fn column_label(key: &str) -> Text {
    match key {
        "name" => Text::Key("settings.column.name"),
        "mode" => Text::Key("settings.column.mode"),
        "size" => Text::Key("settings.column.size"),
        "date" => Text::Key("settings.column.date"),
        "kind" => Text::Key("settings.column.kind"),
        other => {
            let mut chars = other.chars();
            Text::data(chars.next().map_or_else(String::new, |first| first.to_uppercase().collect::<String>() + chars.as_str()))
        }
    }
}

pub fn column_rows(data: &Json) -> Vec<Row> {
    let shown = columns(data);
    let mut out = vec![Row::group("settings.group.columns"), Row { glyph: "file", ..Row::new(Kind::Lock, column_label("name")) }];
    for id in ["mode", "size", "date", "kind"] {
        out.push(Row { id: format!("column:{id}"), on: shown.iter().any(|c| c == id), ..Row::new(Kind::Check, column_label(id)) });
    }
    out.push(Row::action("backView", "settings.row.back_view", "", Text::Key("settings.row.back")));
    out
}

#[cfg(test)]
#[path = "pages_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "pages_places_tests.rs"]
mod places_tests;
