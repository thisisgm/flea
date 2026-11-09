//! Resolve translated labels outside input dispatch and paint/cache-hit paths.
use super::{
    action_group, binding_rows, preset, Binding, Frontend, BASE_SHEET, PRESET_KEYS, SHARED_KEYS,
    SHEET_CAP_BUDGET, SHEET_GROUPS, SHEET_MIRRORS, SHEET_TIE_PRIORITY,
};
use crate::i18n::I18n;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SheetRow {
    pub action: &'static str,
    pub label: String,
    pub keys: String,
    pub context: &'static str,
    pub group: &'static str,
}

// The menu boards draw a Ctrl chord beside these rows alone; every other row shows a bare key or nothing.
const CHORD_HINTS: &[&str] = &["print"];

pub fn hint_for(name: &str, action: &str) -> &'static str {
    let name = preset(name);
    binding_rows(name, Frontend::Gui)
        .filter(|row| {
            action_group(row.action) == action
                && (matches!(row.mods, "text" | "none" | "shift")
                    || (row.mods == "ctrl" && CHORD_HINTS.contains(&action)))
        })
        .min_by_key(|row| {
            (if row.preset == name { 0 } else { 2 })
                + match row.mods {
                    "text" => 0,
                    "none" => 1,
                    _ => 4,
                }
        })
        .map_or("", |row| {
            if row.mods == "text" {
                row.key
            } else {
                row.keys
            }
        })
}

pub fn spelled(keys: &str) -> String {
    let parts: Vec<_> = keys.split('-').collect();
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            let mapped = match *part {
                "ctrl" if index + 1 < parts.len() => "Ctrl",
                "shift" if index + 1 < parts.len() => "Shift",
                "alt" if index + 1 < parts.len() => "Alt",
                "super" if index + 1 < parts.len() => "Super",
                "plus" => "+",
                "minus" => "-",
                "equal" => "=",
                "underscore" => "_",
                "greater" => ">",
                "less" => "<",
                "slash" => "/",
                "period" => ".",
                "comma" => ",",
                _ => "",
            };
            if !mapped.is_empty() {
                mapped.to_owned()
            } else if parts.len() == 1 && part.chars().count() == 1 {
                (*part).to_owned()
            } else {
                let mut chars = part.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().collect::<String>() + chars.as_str()
                })
            }
        })
        .collect::<Vec<_>>()
        .join("+")
}

pub fn key_for(name: &str, action: &str) -> String {
    let hint = hint_for(name, action);
    if !hint.is_empty() {
        return hint.to_owned();
    }
    row_for(name, action, None).map_or_else(String::new, |row| spelled(row.keys))
}

pub fn row_for(name: &str, action: &str, mods: Option<&str>) -> Option<&'static Binding> {
    binding_rows(name, Frontend::Gui)
        .find(|row| row.action == action && mods.map_or(true, |mods| row.mods == mods))
}

pub fn key_of(name: &str, action: &str, mods: &str) -> String {
    row_for(name, action, Some(mods)).map_or_else(String::new, |row| spelled(row.keys))
}

pub fn hinted(catalog: &I18n, template: &str, name: &str, action: &str) -> String {
    let key = key_for(name, action);
    if key.is_empty() {
        String::new()
    } else {
        catalog.tr_args(template, &[&catalog.keycap(&key)])
    }
}

pub fn hinted_all(catalog: &I18n, template: &str, name: &str, actions: &[&str]) -> String {
    let keys: Vec<_> = actions
        .iter()
        .map(|action| catalog.keycap(&key_for(name, action)))
        .collect();
    if keys.iter().any(String::is_empty) {
        return String::new();
    }
    catalog.tr_args(
        template,
        &keys.iter().map(String::as_str).collect::<Vec<_>>(),
    )
}

pub fn sheet(name: &str, frontend: Frontend, catalog: &I18n) -> Vec<SheetRow> {
    let name = preset(name);
    let mut rows: Vec<&Binding> = Vec::new();
    for row in binding_rows(name, frontend) {
        if !rows
            .iter()
            .any(|kept| kept.keys == row.keys && kept.action == row.action)
        {
            rows.push(row);
        }
    }
    let listing_action = |action: &str| {
        PRESET_KEYS.iter().chain(SHARED_KEYS).any(|row| {
            !row.action.is_empty()
                && row.context.split(',').any(|context| context == "listing")
                && action_group(row.action) == action
        })
    };
    rows.extend(PRESET_KEYS.iter().chain(SHARED_KEYS).filter(|row| {
        SHEET_GROUPS
            .iter()
            .any(|group| group.actions.contains(&row.action))
            && !listing_action(action_group(row.action))
            && !row.context.split(',').any(|context| context == "listing")
    }));
    rows.retain(|row| row.sheet);
    let mut result: Vec<SheetRow> = Vec::new();
    let mut singles = Vec::new();
    for row in &rows {
        let action = action_group(row.action);
        if result.iter().any(|kept| kept.action == action) {
            continue;
        }
        let mut spellings: Vec<_> = rows
            .iter()
            .filter(|candidate| action_group(candidate.action) == action)
            .collect();
        spellings.sort_by_key(|candidate| {
            let secondary = if candidate.preset == name {
                usize::MAX
            } else {
                SHEET_TIE_PRIORITY
                    .iter()
                    .filter(|row| row.0 == "all" || row.0 == name)
                    .flat_map(|row| row.1.iter())
                    .position(|key| *key == candidate.keys)
                    .unwrap_or(usize::MAX)
            };
            (
                (if candidate.preset == name { 0 } else { 2 }) + u8::from(candidate.mods != "text"),
                secondary,
            )
        });
        let first = spellings[0].keys;
        let mut keys = first.to_owned();
        for candidate in spellings.iter().skip(1) {
            let length = first.encode_utf16().count() + 3 + candidate.keys.encode_utf16().count();
            if candidate.keys != first && length <= SHEET_CAP_BUDGET {
                keys = format!("{first} / {}", candidate.keys);
                break;
            }
        }
        singles.push(first);
        let label = BASE_SHEET
            .iter()
            .rev()
            .find(|base| action_group(base.action) == action)
            .map_or(row.label_key, |base| base.label_key);
        let group = SHEET_GROUPS
            .iter()
            .find(|group| group.actions.contains(&action))
            .map_or("", |group| group.id);
        result.push(SheetRow {
            action,
            label: catalog.tr(label),
            keys,
            context: row.context,
            group,
        });
    }
    for pair in SHEET_MIRRORS {
        let indices = pair.map(|action| result.iter().position(|row| row.action == action));
        if let [Some(first), Some(second)] = indices {
            if (result[first].keys == singles[first]) != (result[second].keys == singles[second]) {
                result[first].keys = singles[first].to_owned();
                result[second].keys = singles[second].to_owned();
            }
        }
    }
    result
}

#[cfg(test)]
#[path = "presentation_keycap_tests.rs"]
pub(crate) mod keycap_tests;

#[cfg(test)]
#[path = "presentation_chord_tests.rs"]
mod chord_tests;
