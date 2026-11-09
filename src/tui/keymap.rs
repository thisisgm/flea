use super::input::Key;
use std::collections::HashMap;

#[derive(Default)]
pub struct Map {
    blocks: Vec<(String, HashMap<String, String>)>,
}
// The key names the hints print, read from the keymap once per run so a rebound key reaches its hint.
#[derive(Default, Clone)]
pub struct Hints {
    pub keys: String,
    pub load_preview: String,
    pub delete_permanently: String,
    pub trash: String,
    pub undo: String,
}
impl Hints {
    pub fn new(map: &Map, preset: &str) -> Self {
        let chord = |action: &str| map.chord(action, preset, "listing").unwrap_or_default();
        Self {
            keys: chord("keymapSheet"),
            load_preview: chord("loadPreview"),
            delete_permanently: chord("deletePermanently"),
            trash: chord("trash"),
            // A receipt names the plain undo key before any chord, the way the boards draw "z undoes".
            undo: map.chord_where("undo", preset, "listing", plain).or_else(|| map.chord("undo", preset, "listing")).unwrap_or_default(),
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
        // Older shared maps retain the obsolete contents refusal in the optional preset.
        for (kind, block) in &mut map.blocks {
            if kind == "tui" && get(block, "name") == "yazi" && get(block, "prefix").is_empty()
                && get(block, "key") == "S" && get(block, "mods") == "text" && get(block, "context") == "listing"
                && get(block, "action") == "unsupported:Content search is unavailable; s opens built-in filename search" {
                block.insert("action".into(), "searchContents".into());
            }
            if kind == "tui" && get(block, "name") == "yazi" && get(block, "mods") == "shift"
                && get(block, "key") == "Return" && get(block, "context") == "listing"
                && get(block, "action") == "unsupported:Interactive application picking is unavailable in this terminal build; O opens a folder tab" {
                block.insert("action".into(), "menu".into());
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
        if shift_in_text(key) {
            for (kind, block) in &self.blocks {
                if kind == "text" && get(block, "char") == key.text {
                    return get(block, "action").into();
                }
            }
        }
        if key.mods.is_empty() && key.text.len() == 1 && matches!(key.text.as_bytes()[0], b'1'..=b'9') {
            return format!("tab{}", key.text);
        }
        // Convert already has this fallback in dispatch; keep map queries and hints consistent.
        if key.text == "F" && matches!(key.mods.as_str(), "" | "shift") { return "convert".into(); }
        String::new()
    }
    pub fn sequence(&self, prefix: &str, key: &Key, preset: &str, context: &str) -> String {
        let action: String = self.blocks.iter().find(|(kind, block)| kind == "tui" && get(block, "name") == preset
            && get(block, "prefix") == prefix && context_matches(block, context) && matches_key(block, key))
            .map(|(_, block)| get(block, "action").into()).unwrap_or_default();
        if preset == "yazi" && context == "listing" && prefix == "m"
            && action == "unsupported:Specific detail modes are unavailable in this terminal build" {
            let field = match key.text.as_str() { "s" => "size", "p" => "mode", "m" => "modified", "o" => "owner", _ => return action };
            return format!("unsupported:detail:{field}");
        }
        action
    }
    // The first key a preset binds to an action in a context, else its first reachable prefix sequence, spelled as the sheet spells it: "Ctrl+Space", "Escape", "?", "t t".
    pub fn chord(&self, action: &str, preset: &str, context: &str) -> Option<String> { self.chord_where(action, preset, context, |_| true) }
    // The first key or prefix sequence of action whose modifiers keep accepts.
    pub fn chord_where(&self, action: &str, preset: &str, context: &str, keep: fn(&Key) -> bool) -> Option<String> {
        let direct = self.blocks.iter().filter(|(_, block)| get(block, "action") == action)
            .filter_map(|(kind, block)| block_key(kind, block))
            .find(|(_, key)| keep(key) && self.in_context(key, preset, context) == action)
            .map(|(name, key)| spell(&key, name));
        direct.or_else(|| self.blocks.iter().find_map(|(kind, block)| {
            let prefix = get(block, "prefix");
            if kind != "tui" || prefix.is_empty() || get(block, "name") != preset
                || get(block, "action") != action || !context_matches(block, context) { return None; }
            let (tail_name, tail) = block_key(kind, block)?;
            if !keep(&tail) || self.sequence(prefix, &tail, preset, context) != action { return None; }
            let arm = format!("prefix:{prefix}");
            self.blocks.iter().filter(|(_, block)| get(block, "action") == arm)
                .filter_map(|(kind, block)| block_key(kind, block))
                .find(|(_, key)| keep(key) && self.in_context(key, preset, context) == arm)
                .map(|(name, key)| format!("{} {}", spell(&key, name), spell(&tail, tail_name)))
        }))
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
// A key pressed on its own, with no modifier held.
fn plain(key: &Key) -> bool { matches!(key.mods.as_str(), "text" | "code" | "none" | "") }
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
            | "convert"
            | "combine"
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
            | "wordBackward" | "wordForward" | "killBol" | "killEol" | "killBackward" | "killForward"
            | "suspend" | "pasteAbsoluteLink" | "pasteLink" | "pasteHardLink" | "detailBirth" | "detailOff"
            | "copyPath" | "copyUri" | "copydirpath" | "copyDirUri" | "copyName" | "copyStem"
            | "copyRelative" | "copyQuoted"
            | "findPrevious" | "findNext" | "sortBirth" | "sortBirthReverse" | "sortExtension" | "sortRandom" | "followLink"
            | "openTrash"
    )
}
fn matches_key(block: &HashMap<String, String>, key: &Key) -> bool {
    match get(block, "mods") {
        "text" => (key.mods.is_empty() || shift_in_text(key)) && get(block, "key") == key.text,
        mods => (if mods == "none" { "" } else { mods }) == key.mods && get(block, "key") == key.name,
    }
}
// Shift is already in the text only for an uppercase letter; any other Shift stays part of the chord, as input.rs keeps it.
fn shift_in_text(key: &Key) -> bool {
    key.mods == "shift" && key.text.chars().count() == 1 && key.text.chars().all(char::is_uppercase)
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
