// The Places pane, from ui/js/Settings.js placesRows, with the stored favourites read as ui/js/Places.js reads them.
use super::super::rows::{choice, Kind, Row};
use super::super::Text;
use super::{at, is_true, not_false, State};
use crate::jsondoc::{self, Json};

pub const WIDTH_STOPS: [u32; 4] = [160, 192, 224, 256];

// The nearest width stop; anything that is not a finite number is the shipped 192.
pub fn sidebar_width(value: Option<&Json>) -> u32 {
    let Some(px) = value.and_then(|v| match v { Json::Num(_) => v.as_f64(), _ => None }).filter(|px| px.is_finite()) else {
        return 192;
    };
    let mut best = WIDTH_STOPS[0];
    for stop in WIDTH_STOPS.into_iter().skip(1) {
        if (f64::from(stop) - px).abs() < (f64::from(best) - px).abs() {
            best = stop;
        }
    }
    best
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Favourite {
    pub label: String,
    pub stored_path: String,
    pub glyph: &'static str,
    pub error: Text,
}

const SCHEMES: [&str; 8] = ["smb://", "sftp://", "ftp://", "dav://", "davs://", "afp://", "nfs://", "file://"];

// Why a stored record cannot be a favourite, or Empty when it can.
fn record_error(record: &Json) -> Text {
    let Json::Obj(_) = record else { return Text::Key("settings.favourite.invalid_record") };
    let label = record.get("label").and_then(Json::as_str);
    if label.map_or(true, |l| l.trim().is_empty()) {
        return Text::Key("settings.favourite.label_needs_text");
    }
    let Some(path) = record.get("path").and_then(Json::as_str).filter(|p| !p.chars().any(|c| c < ' ' || c == '\u{7f}')) else {
        return Text::Key("settings.favourite.invalid_path");
    };
    if path.starts_with('/') || path == "~" || path.starts_with("~/") || SCHEMES.iter().any(|s| path.len() > s.len() && path.starts_with(s)) {
        return Text::Empty;
    }
    Text::Key("settings.favourite.full_path")
}

pub fn favourite_entries(records: Option<&Json>) -> Vec<Favourite> {
    let Some(records) = records.and_then(Json::as_array) else { return Vec::new() };
    records.iter().map(|record| {
        let label = match record.get("label").and_then(Json::as_str) {
            Some(label) => label.to_owned(),
            None => jsondoc::render(record).trim_end().to_owned(),
        };
        let path = record.get("path").and_then(Json::as_str).unwrap_or("").to_owned();
        Favourite {
            label,
            glyph: if path.contains("://") { "network" } else { "folder" },
            stored_path: path, error: record_error(record),
        }
    }).collect()
}

// Sample input: tilde("/home/gm/P", "/home/gm") is "~/P"; a sibling named like home stays absolute.
pub fn tilde(path: &str, home: &str) -> String {
    if !home.is_empty() && (path == home || path.starts_with(&format!("{home}/"))) {
        return format!("~{}", &path[home.len()..]);
    }
    path.to_owned()
}

pub fn places_rows(state: &State<'_>) -> Vec<Row> {
    let data = state.data;
    let places = |key: &'static str| ["places", key];
    // The one action that governs the whole group rides its heading.
    let mut out = vec![Row { id: "addFavourite".into(), action: "addFavourite", value: Text::Key("settings.places.add_folder"), ..Row::group("settings.group.favorites") }];
    for (index, entry) in favourite_entries(at(data, &places("favourites"))).into_iter().enumerate() {
        let error = if entry.error.is_empty() { state.favourite_statuses.get(index).cloned().unwrap_or_default() } else { entry.error };
        let label = if entry.label.is_empty() { Text::Key("settings.favourite.invalid") } else { Text::Data(entry.label) };
        out.push(Row {
            id: format!("favourite:{index}"), glyph: entry.glyph, display: tilde(&entry.stored_path, state.home),
            value: Text::Data(entry.stored_path), error, favourite: Some(index),
            ..Row::new(Kind::Favourite, label)
        });
    }
    out.push(Row::group("settings.group.built_in"));
    // The last field is what a key the file never stored reads as.
    let builtins = [("showHome", "settings.places.home", "house", true), ("showRecent", "settings.places.recent", "history", false),
        ("showNetwork", "settings.places.network", "network", true), ("showDevices", "settings.places.devices", "drive", true),
        ("showTrash", "settings.places.trash", "trash", true)];
    let rail = [("driveSize", "settings.places.drive_size", "drive", false), ("trashCount", "settings.places.trash_count", "trash", false),
        ("showUnmounted", "settings.places.show_unmounted", "drive", true)];
    let rail_tail = [("showTags", "settings.places.show_tags", "tag", false),
        ("autoHide", "settings.places.auto_hide", "maximize", false)];
    let switch = |(key, label, glyph, shipped): (&'static str, &'static str, &'static str, bool)| {
        let on = if shipped { not_false(data, &places(key)) } else { is_true(data, &places(key)) };
        Row::check(&format!("places.{key}"), label, glyph, on)
    };
    out.extend(builtins.into_iter().map(switch));
    out.push(Row::group("settings.group.sidebar"));
    out.extend(rail.into_iter().map(switch));
    // Hosts list only from the NETWORK heading's plus, so the option greys in place while Network is unticked.
    out.push(Row { available: not_false(data, &places("showNetwork")),
        ..switch(("nearbyHosts", "settings.places.nearby_hosts", "network", false)) });
    out.extend(rail_tail.into_iter().map(switch));
    // The rail is remembered as a word; auto-hide answers the pointer instead, so the choice greys while it is on.
    let shown = at(data, &places("rail")).and_then(Json::as_str) != Some("hidden");
    out.push(Row { available: !is_true(data, &places("autoHide")), ..Row::check("places.rail", "settings.places.show_sidebar", "columns", shown) });
    let width = sidebar_width(at(data, &places("sidebarWidth"))).to_string();
    out.push(choice("places.sidebarWidth", "settings.places.sidebar_width", "maximize", &["160", "192", "224", "256"],
        &["settings.choice.w160", "settings.choice.w192", "settings.choice.w224", "settings.choice.w256"], &width));
    // Permanent deletion is outside the undo journal, so the sweep is off until switched on.
    out.push(Row::group("settings.group.trash"));
    out.push(Row { caption: Text::Key("settings.caption.permanently"), ..Row::check("trashAutoEmpty", "settings.places.auto_empty", "history", is_true(data, &["trashAutoEmpty"])) });
    out
}
