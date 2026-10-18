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
                if kind == "tui" && get(block, "name") == name && get(block, "prefix").is_empty()
                    && context_matches(block, context) && matches_key(block, key) {
                    return get(block, "action").into();
                }
            }
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
        if key.mods == "shift" {
            for (kind, block) in &self.blocks {
                if kind == "text" && get(block, "char") == key.text && !key.text.is_empty() {
                    return get(block, "action").into();
                }
            }
        }
        if key.mods.is_empty() && key.text.len() == 1 && matches!(key.text.as_bytes()[0], b'1'..=b'9') {
            return format!("tab{}", key.text);
        }
        String::new()
    }
    pub fn sequence(&self, prefix: &str, key: &Key, preset: &str, context: &str) -> String {
        self.blocks.iter().find(|(kind, block)| kind == "tui" && get(block, "name") == preset
            && get(block, "prefix") == prefix && context_matches(block, context) && matches_key(block, key))
            .map(|(_, block)| get(block, "action").into()).unwrap_or_default()
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
        let mut resolved = Vec::new();
        for (kind, block) in &self.blocks {
            let Some((name, key)) = block_key(kind, block) else { continue; };
            let prefix = get(block, "prefix");
            let found = if prefix.is_empty() { self.action(&key, preset) }
                else {
                    let start = self.action(&Key::character(prefix.chars().next().unwrap(), ""), preset);
                    if start != format!("prefix:{}", prefix) { continue; }
                    self.sequence(prefix, &key, preset, "listing")
                };
            if found.is_empty() { continue; }
            let chord = format!("{}{}{}", if prefix.is_empty() { String::new() } else { format!("{} ", prefix) },
                spell(&key, name), if found.ends_with("Arm") { name } else { "" });
            let entry = (canonical(&found).to_string(), chord);
            if !resolved.contains(&entry) { resolved.push(entry); }
        }
        let mut rows = Vec::new();
        let mut seen = Vec::new();
        for (_, label) in self.blocks.iter().filter(|(kind, _)| matches!(kind.as_str(), "sheet" | "tuisheet" | "tui")) {
            let action = canonical(get(label, "action"));
            if seen.contains(&action) || get(label, "label").is_empty() || !supported(action) { continue; }
            let keys: Vec<_> = resolved.iter().filter(|(a, _)| a == action).map(|(_, key)| key.as_str()).collect();
            if !keys.is_empty() {
                seen.push(action);
                rows.push(format!("{}  {}", keys.join(" / "), get(label, "label")));
            }
        }
        rows.push("1–9  Select tab".into());
        rows
    }
}
// A block as the key it binds and the name the sheet prints for it; None for a block that binds no key the sheet can show.
fn block_key<'a>(kind: &'a str, block: &'a HashMap<String, String>) -> Option<(&'a str, Key)> {
    let mods = if matches!(kind, "preset" | "tui") {
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
pub(super) fn supported(action: &str) -> bool {
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
            | "quitNoCwd"
            | "editFile"
            | "detailNext"
            | "print"
            | "factsToggle"
            | "pageForward"
            | "reload"
            | "invertSelection"
            | "clearSelection"
            | "selectNext"
            | "clearClipboard"
            | "openAgent"
            | "searchContents"
            | "tabMoveLeft"
            | "tabMoveRight"
            | "halfPageUp"
            | "halfPageDown"
            | "sortName"
            | "sortNameReverse"
            | "sortDate"
            | "sortDateReverse"
            | "sortSize"
            | "sortSizeReverse"
            | "goHome"
            | "goConfig"
            | "goDownloads"
    )
}
fn matches_key(block: &HashMap<String, String>, key: &Key) -> bool {
    match get(block, "mods") {
        "text" => matches!(key.mods.as_str(), "" | "shift") && get(block, "key") == key.text,
        mods => (if mods == "none" { "" } else { mods }) == key.mods && get(block, "key") == key.name,
    }
}
fn context_matches(block: &HashMap<String, String>, context: &str) -> bool {
    get(block, "context").split(',').any(|s| s == context || s == "all")
}
fn get<'a>(map: &'a HashMap<String, String>, key: &str) -> &'a str {
    map.get(key).map(String::as_str).unwrap_or("")
}
#[cfg(test)]
#[path = "keymap_tests.rs"]
mod tests;
