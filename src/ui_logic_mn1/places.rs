//! The Locked tile's, a Places row's and a neighbour column's menus: what each offers and what a choice means.
use super::build::{self, Context, S_IFDIR, S_IFMT, S_IFREG};
use super::entry::{Entry, Text};
use super::inventory::INVENTORY;
use super::rows;

// Rows that act on a locked folder without listing it, in inventory order.
pub const LOCKED_IDS: [&str; 3] = ["openTerminal", "permissions", "copyAs"];
pub const LOCKED_REFUSAL: &str = "menu.locked.refusal";
pub const LOCKED_NOTICE: &str = "menu.locked.terminal_copy_hidden";
// An owner who can read and search the folder was not the one refused, so the denial proves another owner.
const OWNER_CAN_LIST: u32 = 0o500;

// The tile's Copy path row keeps acting at once, while its visibility follows the Copy As switch.
pub fn locked_entries(mode: u32, hidden: &[String]) -> Vec<Entry> {
    let scoped = Context {
        row_mode: mode,
        selection_count: 1,
        ..Context::default()
    };
    let mut out: Vec<Entry> = Vec::new();
    let mut group = "";
    let built = build::build_entries(
        build::Kind::File,
        &Context {
            hidden: Vec::new(),
            ..scoped
        },
    );
    for spec in INVENTORY
        .iter()
        .filter(|spec| LOCKED_IDS.contains(&spec.id) && !rows::is_hidden(hidden, spec.id))
    {
        let Some(mut entry) = built.iter().find(|entry| entry.id == spec.id).cloned() else {
            continue;
        };
        if spec.id == "copyAs" {
            entry = Entry::row("copypath", Text::Key("menu.row.copy_path"), spec.glyph);
        }
        if spec.id == "permissions" {
            let permission = locked_permissions(mode);
            entry.disabled = permission.disabled;
            entry.errored = permission.errored;
            entry.hint = permission.hint;
        }
        if !out.is_empty() && group != spec.group {
            out.push(Entry::separator());
        }
        group = spec.group;
        out.push(entry);
    }
    out
}

pub fn locked_refusal(mode: u32, hidden: &[String]) -> Option<&'static str> {
    locked_entries(mode, hidden)
        .is_empty()
        .then_some(LOCKED_REFUSAL)
}

// The shipped default shows Permissions alone, so the status line still names the two Settings > Menus hides, as 0.4.0 named all three.
pub fn locked_notice(hidden: &[String]) -> Option<&'static str> {
    let shown: Vec<&str> = LOCKED_IDS
        .into_iter()
        .filter(|id| !rows::is_hidden(hidden, id))
        .collect();
    (shown == ["permissions"]).then_some(LOCKED_NOTICE)
}

// Only the owner can chmod it back, so a folder that proves another owner reads red and says so.
pub fn locked_permissions(mode: u32) -> rows::Refusal {
    let kind = mode & S_IFMT;
    if kind != S_IFREG && kind != S_IFDIR {
        return rows::Refusal {
            disabled: true,
            errored: true,
            hint: rows::permissions_word(mode),
        };
    }
    if mode & OWNER_CAN_LIST == OWNER_CAN_LIST {
        return rows::Refusal {
            disabled: true,
            errored: true,
            hint: Some(Text::Key("menu.state.not_yours")),
        };
    }
    rows::Refusal::default()
}

// The key carries the path, because the rail rebuilds on a poll under an open menu.
pub fn place_key(path: &str, favourite_index: Option<usize>) -> String {
    let index = favourite_index.map_or_else(|| "-1".to_owned(), |index| index.to_string());
    format!("place:{index}:{path}")
}

// A Places or Favorites row's folder menu; a hidden list that could not be read switches nothing on.
pub fn place_menu(hidden: Option<&[String]>, favourite: bool) -> Vec<Entry> {
    let Some(hidden) = hidden else {
        return Vec::new();
    };
    build::place_entries(&Context {
        hidden: hidden.to_vec(),
        has_row: true,
        place_favourite: favourite,
        ..Context::default()
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceChoice {
    Open(String),
    OpenTab(String),
    Terminal(String),
    CopyPath(String),
    AddFavourite { path: String, label: String },
    // Removed only while the record at index still names the path the menu opened on.
    RemoveFavourite { index: usize, path: String },
    // The rail's older key: removed only while the record still serializes to the one it opened on.
    RemoveLegacy { index: usize, record: String },
    Ignore,
}

pub fn place_leaf(path: &str) -> &str {
    let trimmed = path.trim_end_matches('/');
    trimmed.rfind('/').map_or(path, |cut| &trimmed[cut + 1..])
}

pub fn place_choice(action: &str, key: &str) -> PlaceChoice {
    if let Some(rest) = key.strip_prefix("favourite:") {
        let Some((index, record)) = rest.split_once(':') else {
            return PlaceChoice::Ignore;
        };
        return match (action, index.parse()) {
            ("removeFavourite", Ok(index)) => PlaceChoice::RemoveLegacy {
                index,
                record: record.to_owned(),
            },
            _ => PlaceChoice::Ignore,
        };
    }
    let Some((index, path)) = key
        .strip_prefix("place:")
        .and_then(|rest| rest.split_once(':'))
    else {
        return PlaceChoice::Ignore;
    };
    if path.is_empty() {
        return PlaceChoice::Ignore;
    }
    let path = path.to_owned();
    match action {
        "open" => PlaceChoice::Open(path),
        "openTab" => PlaceChoice::OpenTab(path),
        "openTerminal" => PlaceChoice::Terminal(path),
        "copypath" => PlaceChoice::CopyPath(path),
        "addFavourite" => PlaceChoice::AddFavourite {
            label: place_leaf(&path).to_owned(),
            path,
        },
        "removeFavourite" => match index.parse() {
            Ok(index) => PlaceChoice::RemoveFavourite { index, path },
            Err(_) => PlaceChoice::Ignore,
        },
        _ => PlaceChoice::Ignore,
    }
}

pub const FAVOURITES_CHANGED: &str = "menu.favourites_changed";

/// What a neighbour column's background open has to wait for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaneBusy {
    pub list_in_flight: bool,
    pub trash_open: bool,
    pub search: bool,
    pub collide_open: bool,
    pub menu_open: bool,
    pub renaming: bool,
    pub filter_typing: bool,
    pub band: bool,
}

// The busy reason that refuses a background intent before any intent is stored, or None.
pub fn can_arm(pane: &PaneBusy) -> Option<&'static str> {
    [
        (pane.list_in_flight, "loading"),
        (pane.trash_open, "trash"),
        (pane.search, "search"),
        (pane.collide_open, "collide"),
        (pane.menu_open, "menu"),
        (pane.renaming, "rename"),
        (pane.filter_typing, "filter"),
        (pane.band, "band"),
    ]
    .into_iter()
    .find_map(|(busy, reason)| busy.then_some(reason))
}

// Sample input: "/home/gm/" is "/home/gm"; the root keeps its slash.
pub fn trim_slash(path: &str) -> &str {
    if path.len() > 1 && path.ends_with('/') {
        &path[..path.len() - 1]
    } else {
        path
    }
}

pub fn same_path(a: &str, b: &str) -> bool {
    trim_slash(a) == trim_slash(b)
}

// Landed rows open the deferred menu only for a successful listing of the drawn target; an empty folder counts.
pub fn should_open(pane_path: &str, pending: &str, listing_state: &str) -> bool {
    !pending.is_empty()
        && same_path(pane_path, pending)
        && matches!(listing_state, "ready" | "empty")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Background {
    Ignored,
    // A busy pane refuses, and only a loading one says so, with the pane's own sentence.
    Refused(&'static str),
    Opened,
    // The pane navigates and the menu waits on its rows at the stored point.
    Navigate(String),
}

pub fn route_background(pane: &PaneBusy, pane_path: &str, base: &str) -> Background {
    if base.is_empty() {
        return Background::Ignored;
    }
    if let Some(busy) = can_arm(pane) {
        return Background::Refused(busy);
    }
    if same_path(base, pane_path) {
        return Background::Opened;
    }
    Background::Navigate(base.to_owned())
}

/// Which menu a rail row opens: the trash and a favourite carry their own handle, every other row a mount's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RailMenu {
    Trash(Vec<Entry>),
    Place { key: String, rows: Vec<Entry> },
    // The Places menu is off: INVENTORY's own Remove from Favorites, never a bare Remove.
    LegacyFavourite { key: String, rows: Vec<Entry> },
    Mount(Vec<Entry>),
    None,
}

pub fn rail_menu_for(
    kind: &str,
    path: &str,
    favourite_index: Option<usize>,
    record: &str,
    trash_total: i64,
    hidden: Option<&[String]>,
    mount: &build::RailEntry,
) -> RailMenu {
    match kind {
        "trash" => RailMenu::Trash(build::trash_entries(trash_total, false)),
        "favourite" | "home" => {
            let index = if kind == "favourite" {
                favourite_index
            } else {
                None
            };
            let rows = place_menu(hidden, index.is_some());
            if !rows.is_empty() {
                return RailMenu::Place {
                    key: place_key(path, index),
                    rows,
                };
            }
            match index.filter(|_| kind == "favourite") {
                Some(index) => RailMenu::LegacyFavourite {
                    key: format!("favourite:{index}:{record}"),
                    rows: build::rail_entries(&build::RailEntry {
                        group: "favourite".into(),
                        kind: "favourite".into(),
                        ..Default::default()
                    }),
                },
                None => RailMenu::None,
            }
        }
        _ => {
            let rows = build::rail_entries(mount);
            if rows.is_empty() {
                RailMenu::None
            } else {
                RailMenu::Mount(rows)
            }
        }
    }
}

/// One saved favourite as the rail holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Favourite {
    pub label: String,
    pub path: String,
}

// The rail's older key serializes the record the way the QML rail did, label first.
pub fn legacy_record(record: &Favourite) -> String {
    format!(
        "{{\"label\":\"{}\",\"path\":\"{}\"}}",
        crate::json::escape(&record.label),
        crate::json::escape(&record.path)
    )
}

// A removal goes ahead only while the record at its index is still the one the menu opened on.
pub fn removal_holds(records: &[Favourite], choice: &PlaceChoice) -> Option<usize> {
    match choice {
        PlaceChoice::RemoveFavourite { index, path } => records
            .get(*index)
            .filter(|r| same_path(&r.path, path))
            .map(|_| *index),
        PlaceChoice::RemoveLegacy { index, record } => records
            .get(*index)
            .filter(|r| legacy_record(r) == *record)
            .map(|_| *index),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceTab {
    // The Trash and the mounts have no folder of their own until opened.
    Nothing,
    // A remote favourite names itself and opens in this tab only.
    Remote(String),
    Open(String),
}

pub const OPENS_HERE: &str = "menu.place.opens_here";

// Sample input: "file:///home/gm/My%20Docs" is "/home/gm/My Docs".
pub fn decode_file_uri(path: &str) -> Option<String> {
    let rest = path.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[at],
            bytes.get(at + 1).copied().and_then(hex),
            bytes.get(at + 2).copied().and_then(hex),
        ) {
            (b'%', Some(hi), Some(lo)) => {
                out.push((hi * 16 + lo) as u8);
                at += 3;
            }
            (byte, _, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

// A middle click on a rail row: a home row or a local favourite opens a new tab on its decoded path.
pub fn place_tab(kind: &str, label: &str, path: &str) -> PlaceTab {
    if kind != "home" && kind != "favourite" || path.is_empty() {
        return PlaceTab::Nothing;
    }
    if path.starts_with('/') {
        return PlaceTab::Open(path.to_owned());
    }
    match decode_file_uri(path) {
        Some(local) if local.starts_with('/') => PlaceTab::Open(local),
        _ => PlaceTab::Remote(label.to_owned()),
    }
}

// Whether a right click at a viewport point lands on bare ground: rows are counted in content space, past the scroll.
pub fn on_background(viewport_y: f32, scroll_y: f32, row_height: f32, rows: usize) -> bool {
    let content = viewport_y + scroll_y;
    content < 0. || content >= row_height * rows as f32
}
