use super::input::Key;
use std::collections::HashMap;

#[derive(Default)]
pub struct Map {
    blocks: Vec<(String, HashMap<String, String>)>,
}
// The key names the hints print, read from the keymap once per run so a rebound key reaches its hint.
#[derive(Default, Clone)]
pub struct Hints {
    pub dismiss: String,
    pub keys: String,
    pub load_preview: String,
    pub delete_permanently: String,
    pub trash: String,
}
impl Hints {
    pub fn new(map: &Map, preset: &str) -> Self {
        let chord = |action: &str| map.chord(action, preset, "listing").unwrap_or_default();
        Self {
            dismiss: chord("escape"),
            keys: chord("keymapSheet"),
            load_preview: chord("loadPreview"),
            delete_permanently: chord("deletePermanently"),
            trash: chord("trash"),
        }
    }
}
impl Map {
    pub fn load() -> Self {
        Self::parse(include_str!("../../keys.toml"))
    }
    // Sample input: [[text]]# comment, followed by char = "j" and action = "cursorDown".
    pub(super) fn parse(text: &str) -> Self {
        let mut map = Self::default();
        for line in text.lines().map(str::trim) {
            if let Some((kind, suffix)) = line
                .strip_prefix("[[")
                .and_then(|header| header.split_once("]]"))
            {
                if suffix.trim().is_empty() || suffix.trim_start().starts_with('#') {
                    map.blocks.push((kind.trim().into(), HashMap::new()));
                    continue;
                }
            }
            if line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let value = value.trim();
                if let Ok(crate::jsondoc::Json::Str(value)) = crate::jsondoc::parse(value) {
                    if let Some((_, block)) = map.blocks.last_mut() {
                        block.insert(key.trim().into(), value);
                    }
                }
            }
        }
        map
    }
    pub fn action(&self, key: &Key, preset: &str) -> String {
        self.in_context(key, preset, "listing")
    }
    pub fn in_context(&self, key: &Key, preset: &str, context: &str) -> String {
        if context == "listing" && key.name == "Insert" && key.mods == "ctrl" {
            return "copy".into();
        }
        if context == "listing" && key.name == "Insert" && key.mods == "shift" {
            return "paste".into();
        }
        if preset == "mac" && key.mods == "ctrl" && key.name == "X" {
            return String::new();
        }
        for name in [preset, "all"] {
            for (kind, block) in &self.blocks {
                let here = get(block, "context");
                let mods = get(block, "mods");
                let key_matches = if mods == "text" {
                    matches!(key.mods.as_str(), "" | "shift") && get(block, "key") == key.text
                } else {
                    (if mods == "none" { "" } else { mods }) == key.mods
                        && get(block, "key") == key.name
                };
                if kind == "preset"
                    && get(block, "name") == name
                    && get(block, "frontend") != "gui"
                    && (here.split(',').any(|part| part == context || part == "all")
                        || (here.is_empty() && context == "listing"))
                    && key_matches
                {
                    return get(block, "action").into();
                }
            }
        }
        if context != "listing" {
            return String::new();
        }
        for (kind, block) in &self.blocks {
            let matches = if key.mods.is_empty() {
                (kind == "text" && get(block, "char") == key.text && !key.text.is_empty())
                    || (kind == "code" && get(block, "key") == key.name)
            } else {
                kind == &key.mods && get(block, "key") == key.name
            };
            if matches {
                return get(block, "action").into();
            }
        }
        String::new()
    }
    // The first key a preset binds to an action in a context, spelled as the sheet spells it: "Ctrl+Space", "Escape", "?".
    pub fn chord(&self, action: &str, preset: &str, context: &str) -> Option<String> {
        self.blocks
            .iter()
            .filter(|(_, block)| get(block, "action") == action)
            .filter_map(|(kind, block)| block_key(kind, block))
            .find(|(_, key)| self.in_context(key, preset, context) == action)
            .map(|(name, key)| spell(&key, name))
    }
    pub fn sheet(&self, preset: &str) -> Vec<String> {
        let mut rows = Vec::new();
        for (_, label) in self.blocks.iter().filter(|(kind, _)| kind == "sheet") {
            let action = get(label, "action");
            if !supported(action) {
                continue;
            }
            let mut keys = Vec::new();
            for (kind, block) in &self.blocks {
                let Some((name, key)) = block_key(kind, block) else {
                    continue;
                };
                let found = self.action(&key, preset);
                if canonical(&found) != action {
                    continue;
                }
                let chord = format!(
                    "{}{}",
                    spell(&key, name),
                    if found.ends_with("Arm") { name } else { "" }
                );
                if !keys.contains(&chord) {
                    keys.push(chord);
                }
            }
            if !keys.is_empty() {
                rows.push(format!("{}  {}", keys.join(" / "), get(label, "label")));
            }
        }
        rows.push("1–9  Select tab".into());
        rows.push("q  Quit".into());
        rows
    }
}
// A block as the key it binds and the name the sheet prints for it; None for a block that binds no key the sheet can show.
fn block_key<'a>(kind: &'a str, block: &'a HashMap<String, String>) -> Option<(&'a str, Key)> {
    let mods = if kind == "preset" {
        get(block, "mods")
    } else {
        kind
    };
    let name = if kind == "text" {
        get(block, "char")
    } else {
        get(block, "key")
    };
    if name.is_empty()
        || !matches!(
            mods,
            "text"
                | "code"
                | "none"
                | ""
                | "shift"
                | "ctrl"
                | "ctrlshift"
                | "alt"
                | "super"
                | "supershift"
                | "superalt"
        )
    {
        return None;
    }
    let key = if mods == "text" {
        Key::character(name.chars().next().unwrap(), "")
    } else {
        Key::named(
            name,
            if matches!(mods, "code" | "none") {
                ""
            } else {
                mods
            },
        )
    };
    Some((name, key))
}
// The key as the sheet and the hints print it: its modifier prefix, then the name the keymap gives it.
fn spell(key: &Key, name: &str) -> String {
    let prefix = match key.mods.as_str() {
        "ctrl" => "Ctrl+",
        "ctrlshift" => "Ctrl+Shift+",
        "alt" => "Alt+",
        "super" => "Super+",
        "supershift" => "Super+Shift+",
        "superalt" => "Super+Alt+",
        "shift" => "Shift+",
        _ => "",
    };
    format!("{}{}", prefix, name)
}
fn canonical(action: &str) -> &str {
    match action {
        "copyArm" => "copy",
        "cutArm" => "cut",
        "pasteArm" => "paste",
        "cursorFirstArm" => "cursorFirst",
        "trashArm" => "trash",
        _ => action,
    }
}
fn supported(action: &str) -> bool {
    matches!(
        action,
        "cursorDown"
            | "cursorUp"
            | "extendDown"
            | "extendUp"
            | "pageDown"
            | "pageUp"
            | "cursorFirst"
            | "cursorLast"
            | "parent"
            | "historyBack"
            | "historyForward"
            | "open"
            | "toggleSelect"
            | "selectAll"
            | "deselectAll"
            | "invertSelection"
            | "toggleHidden"
            | "pathBar"
            | "filter"
            | "search"
            | "rename"
            | "newFolder"
            | "newFile"
            | "properties"
            | "copy"
            | "cut"
            | "paste"
            | "movePaste"
            | "duplicate"
            | "trash"
            | "deletePermanently"
            | "undo"
            | "redo"
            | "sortNext"
            | "sortReverse"
            | "tabNew"
            | "openTab"
            | "openTerminal"
            | "windowNew"
            | "tabClose"
            | "tabNext"
            | "tabPrevious"
            | "preview"
            | "togglePreview"
            | "loadPreview"
            | "focusPreview"
            | "focusNext"
            | "menu"
            | "keymapSheet"
            | "reveal"
            | "escape"
            | "quit"
    )
}
fn get<'a>(map: &'a HashMap<String, String>, key: &str) -> &'a str {
    map.get(key).map(String::as_str).unwrap_or("")
}
#[cfg(test)]
#[path = "keymap_tests.rs"]
mod tests;
