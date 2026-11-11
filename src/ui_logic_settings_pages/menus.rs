// The Menus pane: which menu rows exist, their group masters and the hidden-id set they read, from ui/js/Settings.js.
use super::rows::{Kind, Master, Role, Row};
use super::Text;

pub struct Group {
    pub id: &'static str,
    pub label: &'static str,
    pub ids: &'static [&'static str],
    // Features gate a surface, not a row.
    pub features: &'static [&'static str],
}

pub const BASIC: &[&str] = &["cut", "copy", "paste", "duplicate", "rename", "trash"];

// Storage ids stay stable; a group of one row has no master, which is why Destructive carries neither box nor count.
pub const GROUPS: &[Group] = &[
    Group { id: "basic", label: "settings.menus.group_basic", ids: BASIC, features: &[] },
    Group { id: "destructive", label: "settings.menus.group_destructive", ids: &["delete"], features: &[] },
    Group {
        id: "openInspect", label: "settings.menus.group_open_inspect", features: &[],
        ids: &["openwith", "print", "run", "openAsText", "openTerminal", "openAgent", "moveto", "copyto",
            "properties", "permissions", "makeExecutable", "copyAs", "showOriginal", "tags"],
    },
    Group {
        id: "extras", label: "settings.menus.group_extras", features: &["placeMenu"],
        ids: &["shelf", "compress", "extract", "convert", "flashDrive", "taildrop", "localsend", "rclone", "dropbox",
            "sharelink", "actions", "placeMenu", "updateFlea", "toggleHidden", "toggleLangMarks", "extThumbs", "pasteAs",
            "invertSelection"],
    },
];

// src/uischema.rs DEFAULTS menu.hidden, which ViewState.defaultMenuHidden copies for a document that stores none.
pub const SHIPPED_HIDDEN: &[&str] = &["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "copyAs",
    "pasteAs", "invertSelection", "extThumbs", "run", "openAsText", "openAgent",
    "toggleHidden", "toggleLangMarks"];

// Open draws the lock mark instead of a box: no switch governs it.
pub const LOCKED: &[&str] = &["open"];

const LABELS: &[(&str, &str)] = &[
    ("cut", "settings.menus.cut"), ("copy", "settings.menus.copy"), ("paste", "settings.menus.paste"),
    ("duplicate", "settings.menus.duplicate"), ("rename", "settings.menus.rename"), ("trash", "settings.menus.trash"),
    ("delete", "settings.menus.delete"), ("openwith", "settings.menus.openwith"), ("print", "settings.menus.print"),
    ("run", "settings.menus.run"), ("openAsText", "settings.menus.open_as_text"),
    ("openTerminal", "settings.menus.open_terminal"), ("openAgent", "settings.menus.open_agent"),
    ("moveto", "settings.menus.moveto"), ("copyto", "settings.menus.copyto"), ("properties", "settings.menus.properties"),
    ("permissions", "settings.menus.permissions"), ("makeExecutable", "settings.menus.make_executable"),
    ("copyAs", "settings.menus.copy_as"), ("showOriginal", "settings.menus.show_original"), ("tags", "settings.menus.tags"),
    ("shelf", "settings.menus.shelf"), ("compress", "settings.menus.compress"), ("extract", "settings.menus.extract"),
    ("convert", "settings.menus.convert"), ("flashDrive", "settings.menus.flash_drive"),
    ("taildrop", "settings.menus.taildrop"), ("localsend", "settings.menus.localsend"), ("rclone", "settings.menus.rclone"),
    ("dropbox", "settings.menus.dropbox"), ("sharelink", "settings.menus.sharelink"), ("actions", "settings.menus.run_script"),
    ("placeMenu", "settings.menus.place_menu"), ("updateFlea", "settings.menus.update_flea"),
    ("toggleLangMarks", "settings.menus.lang_marks"), ("extThumbs", "settings.menus.ext_thumbs"),
    ("pasteAs", "settings.menus.paste_as"), ("invertSelection", "settings.menus.invert_selection"),
    ("open", "settings.menus.open"), ("toggleHidden", "settings.menus.toggle_hidden"),
];

// Every row wears the glyph the context menu draws for the same action.
pub const GLYPHS: &[(&str, &str)] = &[
    ("cut", "scissors"), ("copy", "copy"), ("paste", "clipboard"), ("duplicate", "file-plus"), ("rename", "rename"),
    ("trash", "trash"), ("openTerminal", "terminal"), ("copyAs", "file-text"), ("showOriginal", "symlink"),
    ("pasteAs", "symlink"), ("invertSelection", "contrast"), ("permissions", "lock"), ("makeExecutable", "play"),
    ("compress", "archive"), ("delete", "trash"), ("openwith", "app-window"), ("moveto", "folder-plus"),
    ("copyto", "copy"), ("properties", "info"), ("extract", "archive-out"), ("convert", "sliders"),
    ("flashDrive", "drive"), ("sharelink", "network"), ("open", "folder-open"), ("toggleHidden", "eye"),
    ("placeMenu", "folder-open"), ("actions", "zap"), ("updateFlea", "download"), ("toggleLangMarks", "code"),
    ("extThumbs", "image"), ("shelf", "shelf"), ("run", "play"), ("openAsText", "file-text"), ("tags", "tag"),
    ("print", "printer"), ("openAgent", "bot"), ("rclone", "cloud-upload"),
];

// A brand row keeps its service's own mark, never a generic glyph.
pub const MARKS: &[(&str, &str)] = &[
    ("taildrop", "tailscale"), ("localsend", "localsend"), ("dropbox", "dropbox"), ("display.hyprlandIcons", "hyprland"),
];

fn find(table: &[(&str, &'static str)], id: &str) -> Option<&'static str> {
    table.iter().find(|(key, _)| *key == id).map(|(_, value)| *value)
}

pub fn label(id: &str) -> Text {
    find(LABELS, id).map_or_else(|| Text::data(id), Text::Key)
}

pub fn glyph(id: &str) -> &'static str {
    find(GLYPHS, id).unwrap_or("")
}

pub fn mark(id: &str) -> &'static str {
    find(MARKS, id).unwrap_or("")
}

fn hidden_has(hidden: &[String], id: &str) -> bool {
    hidden.iter().any(|h| h == id)
}

// The count is of ENABLED actions, so "5 of 6" means one of the six is switched off.
pub fn enabled(hidden: &[String], list: &[&str]) -> usize {
    list.iter().filter(|id| !hidden_has(hidden, id)).count()
}

pub fn master_state(hidden: &[String], list: &[&str]) -> Master {
    match enabled(hidden, list) {
        on if on == list.len() => Master::All,
        0 => Master::None,
        _ => Master::Some,
    }
}

// A checked master switches its group off, anything else switches it on; ids outside the group survive either way.
pub fn toggle_master(hidden: &[String], list: &[&str]) -> Vec<String> {
    let enable = master_state(hidden, list) != Master::All;
    let mut next: Vec<String> = hidden.iter().filter(|id| !enable || !list.contains(&id.as_str())).cloned().collect();
    if !enable {
        for id in list {
            if !hidden_has(&next, id) {
                next.push((*id).to_owned());
            }
        }
    }
    next
}

pub fn toggle_id(hidden: &[String], id: &str) -> Vec<String> {
    let mut next: Vec<String> = hidden.iter().filter(|h| *h != id).cloned().collect();
    if next.len() == hidden.len() {
        next.push(id.to_owned());
    }
    next
}

pub fn rows(hidden: &[String], key_hints: bool) -> Vec<Row> {
    let mut out = Vec::new();
    for group in GROUPS {
        let master = group.ids.len() > 1;
        let mut head = Row::group(group.label);
        if master {
            head.id = group.id.into();
            head.master = true;
            head.state = master_state(hidden, group.ids);
            let count = |n: usize| Text::data(n.to_string());
            head.value = Text::args("settings.menus.count", vec![count(enabled(hidden, group.ids)), count(group.ids.len())]);
        }
        head.ids = group.ids.to_vec();
        out.push(head);
        for id in group.ids {
            let destructive = *id == "delete";
            out.push(Row {
                id: (*id).into(), label: label(id), glyph: glyph(id), mark: mark(id),
                role: if destructive { Role::Error } else { Role::None },
                value: if destructive { Text::Key("settings.menus.off_by_default") } else { Text::Empty },
                on: !hidden_has(hidden, id),
                ..Row::new(Kind::Check, Text::Empty)
            });
        }
    }
    out.push(Row::group("settings.menus.group_shortcuts"));
    out.push(Row::check("keyHints", "settings.menus.key_hints", "keyboard", key_hints));
    out.push(Row::hint(Text::Key("settings.menus.key_hints_hint")));
    out.push(Row::group("settings.menus.group_always_shown"));
    for id in LOCKED {
        out.push(Row { id: (*id).into(), glyph: glyph(id), ..Row::new(Kind::Lock, label(id)) });
    }
    out
}

#[cfg(test)]
#[path = "menus_tests.rs"]
mod tests;
