//! The rail's section order: places, favorites and tags, then the network and device sections, then plugin sections.
use super::places::sidebar_width;
use super::plugin::Section;
use super::rail::{FavouriteError, Group, Kind, RailEntry};
use super::tags;
use crate::jsondoc::Json;
use std::collections::HashMap;

// The places switches Settings stores; Recent and Tags ship off, Home and Trash on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub home: bool,
    pub recent: bool,
    pub trash: bool,
    pub tags: bool,
}

impl Default for Shown {
    fn default() -> Self {
        Self {
            home: true,
            recent: false,
            trash: true,
            tags: false,
        }
    }
}

// What the state document's places object saves for this unit, read as strictly as RailPlaces.qml and Sidebar.qml read it.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedPlaces {
    pub shown: Shown,
    pub favourites: Vec<Json>,
    pub width: i64,
}

// Sample input: the whole `flea --ui-state` document; only a literal false hides Home or Trash, only a literal true shows Recent or Tags.
pub fn saved_places(state: &Json) -> SavedPlaces {
    let places = state.get("places");
    let key = |name: &str| places.and_then(|p| p.get(name)).and_then(Json::as_bool);
    SavedPlaces {
        shown: Shown {
            home: key("showHome") != Some(false),
            recent: key("showRecent") == Some(true),
            trash: key("showTrash") != Some(false),
            tags: key("showTags") == Some(true),
        },
        favourites: places
            .and_then(|p| p.get("favourites"))
            .and_then(Json::as_array)
            .map(<[Json]>::to_vec)
            .unwrap_or_default(),
        width: sidebar_width(
            places
                .and_then(|p| p.get("sidebarWidth"))
                .and_then(Json::as_f64),
        ),
    }
}

// A section title: a catalog key for a built-in, the plugin's own title for a plugin (empty when it names none, which
// plugin_headings sets under PLUGINS), or none drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Heading {
    None,
    Key(&'static str),
    Plugin(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct RailSection {
    pub heading: Heading,
    pub rows: Vec<RailEntry>,
}

pub fn recent_entry() -> RailEntry {
    RailEntry {
        group: Group::Recent,
        kind: Kind::Recent,
        label_key: "places.row.recent",
        path: "flea:recent".into(),
        glyph: "history",
        ..RailEntry::default()
    }
}

pub fn trash_entry() -> RailEntry {
    RailEntry {
        group: Group::Trash,
        kind: Kind::Trash,
        label_key: "places.row.trash",
        path: "trash:///".into(),
        glyph: "trash",
        ..RailEntry::default()
    }
}

// The TAGS rows list only tags in use, in menu order.
pub fn tag_entries(in_use: &[bool; 7]) -> Vec<RailEntry> {
    tags::rail_tags(in_use)
        .map(|name| RailEntry {
            group: Group::Tag,
            kind: Kind::Tag,
            label_key: tags::label_key(name),
            tag: Some(name),
            glyph: "tag",
            ..RailEntry::default()
        })
        .collect()
}

// A favourite keeps its own record error first and takes an inspection diagnostic only when it has none.
pub fn with_statuses(
    mut favourites: Vec<RailEntry>,
    statuses: &HashMap<usize, String>,
) -> Vec<RailEntry> {
    for entry in &mut favourites {
        if entry.error.is_none() {
            let status = entry
                .favourite_index
                .and_then(|i| statuses.get(&i))
                .filter(|s| !s.is_empty());
            entry.error = status.map(|s| FavouriteError::Status(s.clone()));
        }
    }
    favourites
}

// Sample: Home, Trash; FAVORITES Docs; then NETWORK and DEVICES as their units hand them; then SYNCTHING. Empty sections draw no heading.
pub fn rail_sections(
    home: &[RailEntry],
    shown: Shown,
    favourites: Vec<RailEntry>,
    in_use: &[bool; 7],
    built_in_tail: Vec<RailSection>,
    plugins: &[Section],
) -> Vec<RailSection> {
    let home = if shown.home { home } else { &[] };
    let mut places: Vec<RailEntry> = home.iter().take(1).cloned().collect();
    if shown.recent {
        places.push(recent_entry());
    }
    places.extend(home.iter().skip(1).cloned());
    if shown.trash {
        places.push(trash_entry());
    }
    let tag_rows = if shown.tags {
        tag_entries(in_use)
    } else {
        Vec::new()
    };
    // v0.3.12 Sidebar.qml:499 draws PLACES only over a Home or Trash row, so a lone Recent row stands with no heading.
    let lone_recent = places.iter().all(|e| e.kind == Kind::Recent);
    [
        RailSection {
            heading: if lone_recent {
                Heading::None
            } else {
                Heading::Key("places.heading.places")
            },
            rows: places,
        },
        RailSection {
            heading: Heading::Key("places.heading.favorites"),
            rows: favourites,
        },
        RailSection {
            heading: Heading::Key("places.heading.tags"),
            rows: tag_rows,
        },
    ]
    .into_iter()
    .chain(built_in_tail)
    .chain(plugins.iter().map(|s| RailSection {
        heading: Heading::Plugin(s.heading.clone()),
        rows: s.rows.clone(),
    }))
    .filter(|s| !s.rows.is_empty())
    .collect()
}

// The flat entry list rail keys and the cursor index into, in drawn order.
pub fn flat(sections: &[RailSection]) -> Vec<RailEntry> {
    sections
        .iter()
        .flat_map(|s| s.rows.iter().cloned())
        .collect()
}
