// Keyboard sheet query filter: candidates from the sheet, menus, rail and recent files, ranked per query line.
use super::Text;
use crate::i18n::I18n;
use crate::keys::presentation::SheetRow;
use std::collections::{HashMap, HashSet};

pub use super::sheetkeys::{menu_run_decision, run_decision, Run};

#[cfg(test)]
#[path = "sheet_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sheet_candidates_tests.rs"]
mod candidates_tests;

// The result list is bounded so the sheet stays readable and the cursor starts on the first row.
pub const RESULT_LIMIT: usize = 50;
// The recent-files source reads at most this many paths per query line.
const RECENT_LIMIT: usize = 500;

// Identifies a rail entry so the rail's own index can be found again after a rebuild.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlaceRef {
    pub identity: String,
}

// One row of the query results, before and after the query narrows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Candidate {
    pub label: Text,
    pub name: String,
    pub keys: String,
    pub section: u8,
    pub where_: Text,
    pub action: String,
    pub menu_action: String,
    pub parent_action: String,
    pub menu_label: Text,
    pub disabled: bool,
    // Why a disabled row cannot act, the menu's own state line, drawn muted at the row's end.
    pub reason: Text,
    pub danger: bool,
    pub rail_index: Option<usize>,
    pub path: String,
    pub place: Option<PlaceRef>,
}

// A flyout leaf of a menu entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuLeaf {
    pub id: String,
    pub label: Text,
    pub sheet_label: Text,
    pub disabled: bool,
    pub reason: Text,
    pub separator: bool,
}

// One top-level menu entry; `action` is the entry's action id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuEntry {
    pub label: Text,
    pub action: String,
    pub disabled: bool,
    pub reason: Text,
    pub danger: bool,
    pub separator: bool,
    pub submenu: Vec<MenuLeaf>,
}

// One rail entry; `identity` is the rail's own stable identity string.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailEntry {
    pub label: String,
    pub group: String,
    pub kind: String,
    pub identity: String,
}

// What Enter does with a highlighted row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Disabled,
    Action { action: String },
    Confirm { menu_action: String },
    Menu { menu_action: String },
    Place { rail_index: Option<usize>, identity: String },
    Recent { path: String },
    Nothing,
}

// The matched run of a label that the sheet washes, in characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub length: usize,
}

// The one non-listing context a row carries that the UI names; every other context names no place.
pub fn where_for_context(context: &str) -> Text {
    if context.contains(',') {
        return Text::Empty;
    }
    match context {
        "media" => Text::Key("settings.sheet.where_preview"),
        _ => Text::Empty,
    }
}

// Section 0: the generated sheet rows, already resolved by the keymap presentation.
pub fn action_candidates(rows: &[SheetRow]) -> Vec<Candidate> {
    rows.iter()
        .map(|row| Candidate {
            label: Text::data(row.label.clone()),
            keys: row.keys.clone(),
            section: 0,
            where_: where_for_context(row.context),
            action: row.action.to_string(),
            ..Default::default()
        })
        .collect()
}

// Section 1: the menu's own entries, hidden rows included; flyout leaves follow their parent.
pub fn menu_candidates(entries: &[MenuEntry], hint: impl Fn(&str) -> String) -> Vec<Candidate> {
    let mut out = Vec::new();
    for entry in entries.iter().filter(|entry| !entry.separator) {
        let action = entry.action.as_str();
        out.push(Candidate {
            label: entry.label.clone(),
            keys: hint(action),
            section: 1,
            menu_action: action_with_sub(action, ""),
            disabled: entry.disabled,
            reason: entry.reason.clone(),
            danger: entry.danger,
            ..Default::default()
        });
        for leaf in entry.submenu.iter().filter(|leaf| !leaf.separator) {
            let sheet_wording = !leaf.sheet_label.is_empty();
            out.push(Candidate {
                label: if sheet_wording { leaf.sheet_label.clone() } else { leaf.label.clone() },
                section: 1,
                where_: if sheet_wording { Text::Empty } else { entry.label.clone() },
                menu_action: action_with_sub(action, &leaf.id),
                parent_action: action.to_string(),
                disabled: entry.disabled || leaf.disabled,
                reason: if leaf.reason.is_empty() { entry.reason.clone() } else { leaf.reason.clone() },
                danger: entry.danger,
                ..Default::default()
            });
        }
    }
    out
}

fn action_with_sub(action: &str, leaf: &str) -> String {
    if leaf.is_empty() {
        action.to_string()
    } else {
        format!("{action}:{leaf}")
    }
}

// The group name the rail shows for a place.
fn place_where(group: &str) -> Text {
    match group {
        "favourite" | "favorite" => Text::Key("settings.sheet.where_favorites"),
        "network" => Text::Key("settings.sheet.where_network"),
        "device" => Text::Key("settings.sheet.where_devices"),
        _ => Text::Key("settings.sheet.where_places"),
    }
}

// Section 2: the rail's entries, recent rows excluded; the name is the match key for an exact place.
pub fn place_candidates(rail: &[RailEntry]) -> Vec<Candidate> {
    rail.iter()
        .enumerate()
        .filter(|(_, entry)| !entry.label.is_empty() && entry.kind != "recent")
        .map(|(index, entry)| Candidate {
            label: Text::args("settings.sheet.open_place", vec![Text::data(entry.label.clone())]),
            name: entry.label.clone(),
            section: 2,
            where_: place_where(&entry.group),
            rail_index: Some(index),
            place: Some(PlaceRef { identity: entry.identity.clone() }),
            ..Default::default()
        })
        .collect()
}

// Section 3: recent files, read once per query line and bounded by RECENT_LIMIT.
pub fn recent_candidates(paths: &[String], home: &str) -> Vec<Candidate> {
    let bound = paths.len().min(RECENT_LIMIT);
    paths[..bound]
        .iter()
        .filter(|path| !path.is_empty())
        .map(|path| Candidate {
            label: Text::args("settings.sheet.open_place", vec![Text::data(name_of(path))]),
            section: 3,
            where_: Text::data(tilde(&location_of(path), home)),
            path: path.clone(),
            ..Default::default()
        })
        .collect()
}

// The last path segment.
fn name_of(path: &str) -> &str {
    match path.rfind('/') {
        Some(cut) => &path[cut + 1..],
        None => path,
    }
}

// The parent directory of a path, always absolute; "/" for a root-level file.
fn location_of(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    match path.rfind('/') {
        None | Some(0) => "/".to_string(),
        Some(cut) => {
            let parent = &path[..cut];
            if parent.starts_with('/') {
                parent.to_string()
            } else {
                format!("/{parent}")
            }
        }
    }
}

// Replaces the home prefix with "~" when the path is home or under it.
fn tilde(path: &str, home: &str) -> String {
    match path.strip_prefix(home) {
        Some(rest) if !home.is_empty() && (rest.is_empty() || rest.starts_with('/')) => format!("~{rest}"),
        _ => path.to_string(),
    }
}

// The matched run of the query in the label: case-insensitive substring, None when none or blank.
pub fn match_of(label: &str, query: &str) -> Option<Match> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }
    let text = label.to_lowercase();
    let at = text.find(&needle)?;
    Some(Match { start: text[..at].chars().count(), length: needle.chars().count() })
}

// One action, one row: the key row keeps its place and takes the menu's wording and verdict.
pub fn unique(candidates: &[Candidate]) -> Vec<Candidate> {
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        let run = if candidate.action.is_empty() { candidate.menu_action.clone() } else { candidate.action.clone() };
        if run.is_empty() {
            out.push(candidate.clone());
            continue;
        }
        match at.get(&run).copied() {
            None => {
                at.insert(run, out.len());
                out.push(candidate.clone());
            }
            Some(slot) => {
                if candidate.section == 1 && out[slot].section == 0 {
                    let held = &mut out[slot];
                    held.menu_label = candidate.label.clone();
                    held.menu_action = candidate.menu_action.clone();
                    if candidate.disabled {
                        held.disabled = true;
                        held.reason = candidate.reason.clone();
                    }
                }
            }
        }
    }
    out
}

// The row as the query reads it: the menu wording when only the menu wording matches.
fn worded(row: &Candidate, needle: &str, catalog: &I18n) -> Candidate {
    let label = row.label.resolve(catalog).to_lowercase();
    let menu = row.menu_label.resolve(catalog).to_lowercase();
    if !label.contains(needle) && menu.contains(needle) {
        Candidate { label: row.menu_label.clone(), ..row.clone() }
    } else {
        row.clone()
    }
}

// The sheet's own row matches its key or a word's start in its name, so the field boards' queries stay as drawn.
fn label_contains(row: &Candidate, label: &str, needle: &str) -> bool {
    if row.action != "keymapSheet" {
        return label.contains(needle);
    }
    label.match_indices(needle).any(|(at, _)| label[..at].chars().last().is_none_or(|before| !before.is_alphanumeric()))
}

// Ranks the candidates for one query line: exact name, exact label, substring label, then keys.
pub fn rank(candidates: &[Candidate], query: &str, catalog: &I18n) -> Vec<Candidate> {
    let needle = query.trim().to_lowercase();
    let list = unique(candidates);
    if needle.is_empty() {
        return list.into_iter().take(RESULT_LIMIT).collect();
    }
    let mut exact_place = Vec::new();
    let mut exact = Vec::new();
    let mut matched = Vec::new();
    let mut keyed = Vec::new();
    for row in &list {
        let label = row.label.resolve(catalog).to_lowercase();
        let menu_label = row.menu_label.resolve(catalog).to_lowercase();
        let name = row.name.to_lowercase();
        if !name.is_empty() && name == needle {
            exact_place.push(row.clone());
        } else if label == needle || menu_label == needle {
            exact.push(worded(row, &needle, catalog));
        } else if label_contains(row, &label, &needle) || menu_label.contains(&needle) {
            matched.push(worded(row, &needle, catalog));
        } else if !row.keys.is_empty() && row.keys.to_lowercase().contains(&needle) {
            keyed.push(row.clone());
        }
    }
    let rows: Vec<Candidate> =
        exact_place.into_iter().chain(exact).chain(matched).chain(keyed).take(RESULT_LIMIT).collect();
    without_listed_parents(rows)
}

// A flyout parent whose leaves the bounded list shows is not itself a row.
fn without_listed_parents(rows: Vec<Candidate>) -> Vec<Candidate> {
    let listed: HashSet<String> =
        rows.iter().filter(|row| !row.parent_action.is_empty()).map(|row| row.parent_action.clone()).collect();
    rows.into_iter()
        .filter(|row| !(row.section == 1 && row.parent_action.is_empty() && listed.contains(&row.menu_action)))
        .collect()
}

// Enter runs the highlighted row as its own surface would.
pub fn dispatch(row: &Candidate) -> Decision {
    if row.disabled {
        return Decision::Disabled;
    }
    if row.section == 0 && !row.action.is_empty() {
        return Decision::Action { action: row.action.clone() };
    }
    if row.section == 1 && !row.menu_action.is_empty() {
        return if row.danger {
            Decision::Confirm { menu_action: row.menu_action.clone() }
        } else {
            Decision::Menu { menu_action: row.menu_action.clone() }
        };
    }
    if let (2, Some(place)) = (row.section, &row.place) {
        return Decision::Place { rail_index: row.rail_index, identity: place.identity.clone() };
    }
    if row.section == 3 && !row.path.is_empty() {
        return Decision::Recent { path: row.path.clone() };
    }
    Decision::Nothing
}

// The rail row an opened place points at, found by identity so duplicate labels never swap.
pub fn place_index(entries: &[RailEntry], decided: &Decision) -> Option<usize> {
    let (rail_index, want) = match decided {
        Decision::Place { rail_index, identity } => (*rail_index, identity.as_str()),
        _ => return None,
    };
    if want.is_empty() {
        return None;
    }
    if let Some(at) = rail_index {
        if at < entries.len() && entries[at].identity == want {
            return Some(at);
        }
    }
    entries.iter().position(|entry| entry.identity == want)
}
