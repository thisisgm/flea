use super::input::Key;
use std::collections::HashMap;

#[derive(Default)]
pub struct Map {
    blocks: Vec<(String, HashMap<String, String>)>,
}
impl Map {
    pub fn load() -> Self {
        Self::parse(include_str!("../../keys.toml"))
    }
    // Sample input: [[text]]# comment, followed by char = "j" and action = "cursorDown".
    fn parse(text: &str) -> Self {
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
    pub fn sheet(&self, preset: &str) -> Vec<String> {
        let mut rows = Vec::new();
        for (_, label) in self.blocks.iter().filter(|(kind, _)| kind == "sheet") {
            let action = get(label, "action");
            if !supported(action) {
                continue;
            }
            let mut keys = Vec::new();
            for (kind, block) in &self.blocks {
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
                    continue;
                }
                let text_key = mods == "text";
                let key = if text_key {
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
                let found = self.action(&key, preset);
                if canonical(&found) != action {
                    continue;
                }
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
                let chord = format!(
                    "{}{}{}",
                    prefix,
                    name,
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
mod tests {
    use super::*;
    #[test]
    fn compiled_source_drives_text_and_native_ctrl() {
        let map = Map::load();
        assert_eq!(
            map.action(
                &Key {
                    name: "J".into(),
                    text: "j".into(),
                    mods: "".into(),
                    pointer: None,
                },
                "default"
            ),
            "cursorDown"
        );
        assert_eq!(
            map.action(
                &Key {
                    name: "X".into(),
                    text: "X".into(),
                    mods: "ctrl".into(),
                    pointer: None,
                },
                "mac"
            ),
            ""
        );
        assert_eq!(
            map.action(
                &Key {
                    name: "Insert".into(),
                    text: "".into(),
                    mods: "shift".into(),
                    pointer: None,
                },
                "vim"
            ),
            "paste"
        );
    }
    #[test]
    fn contexts_share_only_declared_actions_and_sheet_matches_preset() {
        let map = Map::load();
        assert_eq!(map.in_context(&Key::character('d', ""), "vim", "pdf"), "");
        assert_eq!(
            map.in_context(&Key::named("Space", ""), "mac", "pdf"),
            "preview"
        );
        assert_eq!(
            map.in_context(&Key::named("Tab", "shift"), "windows", "media"),
            "focusPrevious"
        );
        assert_eq!(
            map.in_context(&Key::named("Insert", "ctrl"), "default", "editor"),
            ""
        );
        assert_eq!(map.action(&Key::character('y', ""), "vim"), "copyArm");
        assert!(map.sheet("vim").iter().any(|line| line.contains("yy")));
        assert!(!map
            .sheet("default")
            .iter()
            .any(|line| line.to_lowercase().contains("grid")));
    }
    #[test]
    fn commented_headers_keep_shipped_mac_navigation_aliases_separate() {
        let map = Map::load();
        for (key, action) in [("Up", "parent"), ("Down", "open"), ("Delete", "trash")] {
            assert_eq!(map.action(&Key::named(key, "ctrl"), "mac"), action);
        }
        assert_eq!(map.action(&Key::character('3', "ctrl"), "mac"), "viewGrid");
    }
    #[test]
    fn header_comments_do_not_strip_quoted_hashes_or_accept_other_suffixes() {
        let map = Map::parse(r##"
[[text]] # A header comment can contain = and ]].
char = "#"
action = "literal#action"
# [[preset]] is still a comment.
[[text]]# No intervening space is required.
char = "j"
action = "cursorDown"
[[code]] unexpected
"##);
        assert_eq!(map.blocks.len(), 2);
        assert_eq!(map.action(&Key::character('#', ""), "default"), "literal#action");
        assert_eq!(map.action(&Key::character('j', ""), "default"), "cursorDown");
    }
}
