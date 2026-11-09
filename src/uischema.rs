// The shipped ui.json shape and the rule each key is measured against; src/uistate.rs applies them.
use crate::jsondoc::{self, Json};

// docs/flea-0.1.4-build-handoff.md section 1's shape and defaults, but density compact (0.3.2), showUnmounted on (0.3.3).
pub const DEFAULTS: &str = r#"{
  "view": "list",
  "pickerView": "list",
  "density": "compact",
  "columns": ["name", "size", "date"],
  "columnsLimit": 3,
  "columnWidths": {},
  "addressBar": "breadcrumb",
  "sort": { "key": "name", "reverse": false },
  "rememberSort": true,
  "folderSorts": {},
  "rememberViewPerFolder": false,
  "folderViews": {},
  "dual": { "paths": [], "focus": 0, "split": 5000 },
  "foldersFirst": true,
  "groupByKind": false,
  "hidden": false,
  "langMarks": true,
  "hiddenLast": false,
  "showGitStatus": false,
  "highlightToday": false,
  "wrapAtEnds": false,
  "escapeUp": false,
  "openMode": "double",
  "clickRename": true,
  "openFoldersWhileDragging": false,
  "keyHints": false,
  "startIn": "home",
  "startFolder": "",
  "lastPath": "",
  "lastTabs": {"paths": [], "index": 0},
  "newTab": "current",
  "trashAutoEmpty": false,
  "trashSweptOn": 0,
  "places": {
    "favourites": [],
    "showHome": true, "showRecent": false, "showNetwork": true,
    "showDevices": true, "showTrash": true,
    "driveSize": false, "trashCount": false, "showUnmounted": true, "nearbyHosts": false, "showTags": false, "rail": "shown", "autoHide": false, "sidebarWidth": 192
  },
  "shelf": {
    "enabled": false, "bar": true, "rail": "off",
    "screenshots": true, "recordings": true, "recent": 3
  },
  "preview": {
    "column": true, "loadOn": "automatic",
    "thumbnails": "media", "thumbSize": "medium",
    "thumbNetwork": false, "thumbPhone": false, "thumbUsb": true,
    "prefetchOnHover": false,
    "ctrlZoom": true, "volume": 100, "width": 0, "tracks": [], "wrap": false,
    "preloadAdjacent": true, "mediaControls": true,
    "infoPanel": false, "hideNavbarFullscreen": true, "navbarHideMs": 4000
  },
  "previewFacts": "compact",
  "keys": "default",
  "display": { "textSize": { "mode": "system" }, "hyprlandIcons": false },
  "menu": { "hidden": ["delete", "openTerminal", "placeMenu", "runScript",
            "moveto", "copyto", "copyAs", "pasteAs",
            "invertSelection", "extThumbs", "run", "openAsText", "openAgent",
            "toggleHidden", "toggleLangMarks"] },
  "updates": { "autoCheck": true },
  "plugins": {},
  "menuVersion": 1,
  "stateVersion": 4
}"#;

// The list row's optional columns in the order ui/js/Columns.js lays them out; name is never optional.
pub const OPTIONAL_COLUMNS: [&str; 4] = ["mode", "size", "date", "kind"];

// Omarchy's own textSizeStops, so an override can never land on a size the OEM panel could not produce.
pub const TEXT_SIZE_STOPS: [f64; 7] = [9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 20.0];
// A rail narrower than a mark plus a label is not a rail, and one wider than this is a second pane.
pub const SIDEBAR_STOPS: [f64; 4] = [160.0, 192.0, 224.0, 256.0];

// What a value has to be for the key to keep it. A key that fails its rule falls back to its default.
pub enum Rule {
    Bool,
    Word(&'static [&'static str]),
    // columns names what the list row SHOWS, so it holds each column key at most once and name always.
    Columns,
    Favourites,
    // One place, or "" for a folder the operator has not chosen and a path nothing has recorded yet.
    Place,
    // Tabs040 callout 2: every open tab's folder in order, with the current tab's index.
    LastTabs,
    SidebarWidth,
    // dual.paths is the pair handoff 5a specifies, or the empty array that means nothing remembered.
    Pair,
    // menu.hidden is deliberately open: a closed list would make this Flea drop an id a newer one hid.
    Ids,
    Count(f64, f64),
    TextSize,
    // ListColumns040: a dragged edge per column, clamped to the rails ui/js/Columns.js draws.
    ColumnWidths,
    // Columns hairlines: each column's share left of the preview in ten-thousandths, as dual.split, at most four.
    Tracks,
    // Any whole number, so a newer Flea's higher stamp survives this one's write; no patch may set it.
    Version,
    // A folder path to its own sort order, at most MAX_FOLDER_SORTS entries; see folderSorts above.
    FolderSorts,
    // A folder path to one browser view, at most MAX_FOLDER_VIEWS entries; see folderViews above.
    FolderViews,
    // A plugin id to its switch and raw settings; src/uiplugins.rs keeps the map's shape.
    Plugins,
    Group(&'static [(&'static str, Rule)]),
}

// The top-level keys that record which of src/uimigrate.rs's one-time changes a file has had.
pub const STATE_VERSION: &str = "stateVersion";
pub const MENU_VERSION: &str = "menuVersion";

pub const COLUMN_KEYS: &[&str] = &["name", "mode", "size", "date", "kind"];

// ListColumns040: the four resizable list columns and the rails a drag clamps to, the same pair ui/js/Columns.js MIN_LIST_WIDTH and MAX_LIST_WIDTH name.
pub const COLUMN_WIDTH_KEYS: [&str; 4] = ["mode", "size", "date", "kind"];
pub const COLUMN_WIDTH_MIN: f64 = 48.0;
pub const COLUMN_WIDTH_MAX: f64 = 480.0;

pub const SORT: &[(&str, Rule)] = &[("key", Rule::Word(&SORT_KEYS)), ("reverse", Rule::Bool)];

// A folder's own sort in sort's own shape; the map holds the 500 most recent folders, oldest first.
pub const SORT_KEYS: [&str; 4] = ["name", "size", "date", "kind"];
// Tabs040 callout 2: at most one entry per tab ui/js/Tabs.js MAX allows, in tab order.
pub const MAX_LAST_TABS: usize = 9;
pub const MAX_FOLDER_SORTS: usize = 500;
pub const MAX_FOLDER_VIEWS: usize = 500;

pub const DUAL: &[(&str, Rule)] = &[("paths", Rule::Pair), ("focus", Rule::Count(0.0, 1.0)), ("split", Rule::Count(0.0, 10000.0))];

pub const PLACES: &[(&str, Rule)] = &[
    ("favourites", Rule::Favourites),
    ("showHome", Rule::Bool),
    // Sidebar040: the Recent place ships off, so a shared screen never names recent files.
    ("showRecent", Rule::Bool),
    ("showNetwork", Rule::Bool),
    ("showDevices", Rule::Bool),
    ("showTrash", Rule::Bool),
    ("driveSize", Rule::Bool),
    ("trashCount", Rule::Bool),
    ("showUnmounted", Rule::Bool),
    // 0.4.1: off, so the NETWORK plus opens Add a Network Share as 0.4.0 does.
    ("nearbyHosts", Rule::Bool),
    ("showTags", Rule::Bool),
    ("rail", Rule::Word(&["shown", "hidden"])),
    ("autoHide", Rule::Bool),
    ("sidebarWidth", Rule::SidebarWidth),
];

// The shelf's own six. SettingsRest rule 3: the panel offers 1, 2, 3 or 5 and both kinds off is what
// removes the group, so the range here is only what a hand-edited file may leave behind.
pub const SHELF: &[(&str, Rule)] = &[
    ("enabled", Rule::Bool),
    ("bar", Rule::Bool),
    ("rail", Rule::Word(&["off", "left", "right", "bottom"])),
    ("screenshots", Rule::Bool),
    ("recordings", Rule::Bool),
    ("recent", Rule::Count(0.0, 6.0)),
];

#[path = "uipreview_schema.rs"]
mod preview_schema;
pub use preview_schema::PREVIEW;

// mode is "system" or one stop, so there is nowhere to put a free number; see the handoff's Display row.
pub const TEXT_SIZE: &[(&str, Rule)] = &[("mode", Rule::TextSize)];

// textSize alone. Window opacity, icon theme and shadows are the compositor's, and Flea mirrors it
// rather than carrying a second writable copy of a setting Hyprland already owns.
pub const DISPLAY: &[(&str, Rule)] = &[("textSize", Rule::Group(TEXT_SIZE)), ("hyprlandIcons", Rule::Bool)];

// hidden is the whole of the Menus section's state: the master row SettingsMenus draws over the six
// basic actions derives from it by masterState in ui/js/Settings.js, and cannot disagree with it.
pub const MENU: &[(&str, Rule)] = &[("hidden", Rule::Ids)];

// Settings > About's "Check automatically": on, it governs About opening and the 6 hour poll, never a press.
pub const UPDATES: &[(&str, Rule)] = &[("autoCheck", Rule::Bool)];

pub const SCHEMA: &[(&str, Rule)] = &[
    ("view", Rule::Word(&["list", "columns", "grid", "dual"])),
    // The file chooser's own last-used view, separate from the main window's: a grid
    // picker must not flip the browser, and a list browser must not flip the picker.
    ("pickerView", Rule::Word(&["list", "grid"])),
    ("density", Rule::Word(&["tight", "compact", "normal", "comfortable"])),
    ("columns", Rule::Columns),
    // ColumnsWidth board: 2 to 5 columns from the window width, capped here, shipping at 3.
    ("columnsLimit", Rule::Count(2.0, 5.0)),
    ("columnWidths", Rule::ColumnWidths),
    ("addressBar", Rule::Word(&["path", "breadcrumb"])),
    ("sort", Rule::Group(SORT)),
    ("rememberSort", Rule::Bool),
    ("folderSorts", Rule::FolderSorts),
    ("rememberViewPerFolder", Rule::Bool),
    ("folderViews", Rule::FolderViews),
    ("dual", Rule::Group(DUAL)),
    ("foldersFirst", Rule::Bool),
    ("groupByKind", Rule::Bool),
    ("hidden", Rule::Bool),
    ("langMarks", Rule::Bool),
    ("hiddenLast", Rule::Bool),
    ("showGitStatus", Rule::Bool),
    ("highlightToday", Rule::Bool),
    ("wrapAtEnds", Rule::Bool),
    // ClickAndRefresh: Escape climbs when on, the pointer's open mode, and the
    // slow-click rename it greys. Single click and Escape-up ship off, slow click ships on.
    ("escapeUp", Rule::Bool),
    ("openMode", Rule::Word(&["double", "single"])),
    ("clickRename", Rule::Bool),
    ("openFoldersWhileDragging", Rule::Bool),
    // The Menus section's "Show keyboard hints" row: every menu's key column and the empty
    // directory's own tip, off until it is switched on.
    ("keyHints", Rule::Bool),
    // Where a window opens, and where a new tab opens. "folder" reads startFolder, "last" reads
    // lastPath, which ui/WindowBody.qml writes as the pane moves and no panel control ever touches.
    ("startIn", Rule::Word(&["home", "last", "folder"])),
    ("startFolder", Rule::Place),
    ("lastPath", Rule::Place),
    ("lastTabs", Rule::LastTabs),
    ("newTab", Rule::Word(&["current", "home", "start"])),
    // Settings > Places > Trash. The sweep is off until the operator switches it on, and the day it
    // last ran is whole days since the epoch, which is what keeps it to once a day across launches.
    ("trashAutoEmpty", Rule::Bool),
    ("trashSweptOn", Rule::Count(0.0, 4000000.0)),
    ("places", Rule::Group(PLACES)),
    // Settings > Shelf, which the bar plugin reads from this file and never writes; SettingsRest
    // rules 1 to 4 and ledger directive 59.
    ("shelf", Rule::Group(SHELF)),
    ("preview", Rule::Group(PREVIEW)),
    // TallPreviews: the tall kinds' one compact facts line, or the full table `i` opens; ui/ViewState.qml reads it.
    ("previewFacts", Rule::Word(&["compact", "full"])),
    // SettingsKeys.html's four-value chooser over ui/js/Keymap.js's shared tables. A stored name
    // this build cannot honour falls back to default, which is also what a fresh ui.json holds.
    ("keys", Rule::Word(&["default", "vim", "mac", "windows"])),
    ("display", Rule::Group(DISPLAY)),
    ("menu", Rule::Group(MENU)),
    ("updates", Rule::Group(UPDATES)),
    ("plugins", Rule::Plugins),
    // Not a setting: 0.4.1's menu stamp, apart from stateVersion because 0.4.0 refuses a stateVersion above 4.
    (MENU_VERSION, Rule::Version),
    // Not a setting: the stamp that makes each migration once, for the window, the TUI and the CLI alike.
    (STATE_VERSION, Rule::Version),
];

pub fn defaults() -> Json {
    jsondoc::parse(DEFAULTS).expect("the shipped ui.json defaults are valid JSON")
}

#[cfg(test)]
fn default_names() -> Vec<String> {
    names(&defaults(), SCHEMA)
}

#[cfg(test)]
fn rule_names(schema: &[(&str, Rule)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (key, rule) in schema {
        out.push((*key).to_string());
        if let Rule::Group(sub) = rule {
            for name in rule_names(sub) {
                out.push(format!("{}.{}", key, name));
            }
        }
    }
    out
}

#[cfg(test)]
fn names(value: &Json, schema: &[(&str, Rule)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (key, inner) in value.as_object().unwrap_or(&[]) {
        out.push(key.clone());
        if let Some(Rule::Group(sub)) = schema.iter().find(|(k, _)| k == key).map(|(_, r)| r) {
            for name in names(inner, sub) {
                out.push(format!("{}.{}", key, name));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_the_shipped_shape_key_for_key() {
        let d = defaults();
        let keys: Vec<&str> = d.as_object().expect("object").iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "view", "pickerView", "density", "columns", "columnsLimit", "columnWidths", "addressBar", "sort", "rememberSort", "folderSorts", "rememberViewPerFolder", "folderViews",
                "dual",
                "foldersFirst",
                "groupByKind",
                "hidden",
                "langMarks",
                "hiddenLast",
                "showGitStatus",
                "highlightToday",
                "wrapAtEnds",
                "escapeUp",
                "openMode",
                "clickRename",
                "openFoldersWhileDragging",
                "keyHints",
                "startIn",
                "startFolder",
                "lastPath", "lastTabs", "newTab", "trashAutoEmpty", "trashSweptOn", "places", "shelf",
                "preview", "previewFacts", "keys",
                "display", "menu", "updates", "plugins", "menuVersion", "stateVersion"
            ]
        );
        assert_eq!(d.get(STATE_VERSION).and_then(Json::as_f64), Some(4.0), "a fresh document is already stamped");
        assert_eq!(d.get(MENU_VERSION).and_then(Json::as_f64), Some(1.0), "and carries the menu stamp beside it");
        assert_eq!(d.get("view").and_then(Json::as_str), Some("list"));
        assert_eq!(d.get("pickerView").and_then(Json::as_str), Some("list"));
        assert_eq!(d.get("density").and_then(Json::as_str), Some("compact"));
        assert_eq!(d.get("addressBar").and_then(Json::as_str), Some("breadcrumb"));
        assert_eq!(d.get("keys").and_then(Json::as_str), Some("default"));
        assert_eq!(d.get("foldersFirst").and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("groupByKind").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("hidden").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("hiddenLast").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("langMarks").and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("showGitStatus").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("highlightToday").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("wrapAtEnds").and_then(Json::as_bool), Some(false));
        // Escape stays put and a double click opens as in 0.3.4; slow click renames, on by the defaults ledger.
        assert_eq!(d.get("escapeUp").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("openMode").and_then(Json::as_str), Some("double"));
        assert_eq!(d.get("clickRename").and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("keyHints").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("startIn").and_then(Json::as_str), Some("home"));
        assert_eq!(d.get("startFolder").and_then(Json::as_str), Some(""));
        assert_eq!(d.get("lastPath").and_then(Json::as_str), Some(""));
        assert_eq!(d.get("lastTabs").and_then(|t| t.get("paths")).and_then(Json::as_array).map(<[Json]>::len), Some(0));
        assert_eq!(d.get("lastTabs").and_then(|t| t.get("index")).and_then(Json::as_f64), Some(0.0));
        assert_eq!(d.get("newTab").and_then(Json::as_str), Some("current"));
        assert_eq!(d.get("trashAutoEmpty").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("trashSweptOn").and_then(Json::as_f64), Some(0.0));
        let cols: Vec<&str> = d.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(cols, ["name", "size", "date"]);
        // ColumnsWidth ships at 3, so a default install holds 3 columns from 900 px up.
        assert_eq!(d.get("columnsLimit").and_then(Json::as_f64), Some(3.0));
        assert_eq!(d.get("columnWidths").and_then(Json::as_object).map(<[(String, Json)]>::len), Some(0));
        assert_eq!(d.get("sort").and_then(|s| s.get("key")).and_then(Json::as_str), Some("name"));
        assert_eq!(d.get("sort").and_then(|s| s.get("reverse")).and_then(Json::as_bool), Some(false));
        // Sorting release: hidden files keep today's order, and each folder remembers its sort.
        assert_eq!(d.get("hiddenLast").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("rememberSort").and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("folderSorts").and_then(Json::as_object).map(<[(String, Json)]>::len), Some(0));
        assert_eq!(d.get("rememberViewPerFolder").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("folderViews").and_then(Json::as_object).map(<[(String, Json)]>::len), Some(0));
        assert_eq!(d.get("dual").and_then(|s| s.get("paths")).and_then(Json::as_array).map(<[Json]>::len), Some(0));
        assert_eq!(d.get("dual").and_then(|s| s.get("focus")).and_then(Json::as_f64), Some(0.0));
        // The left listing's share of the room right of the rail, in ten-thousandths: 5000 is the equal split 0.3.9 lays out.
        assert_eq!(d.get("dual").and_then(|s| s.get("split")).and_then(Json::as_f64), Some(5000.0));
        // The shelf ships off, and its switch is what installs the plugin.
        assert_eq!(d.get("shelf").and_then(|s| s.get("enabled")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("shelf").and_then(|s| s.get("bar")).and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("shelf").and_then(|s| s.get("rail")).and_then(Json::as_str), Some("off"));
        assert_eq!(d.get("shelf").and_then(|s| s.get("recent")).and_then(Json::as_f64), Some(3.0));
        assert_eq!(d.get("shelf").and_then(|s| s.get("screenshots")).and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("shelf").and_then(|s| s.get("recordings")).and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("places").and_then(|p| p.get("sidebarWidth")).and_then(Json::as_f64), Some(192.0));
        // RailAdditions rule 4: the rail is a remembered state, and a fresh home remembers it shown.
        assert_eq!(d.get("places").and_then(|p| p.get("rail")).and_then(Json::as_str), Some("shown"));
        assert_eq!(d.get("places").and_then(|p| p.get("autoHide")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("places").and_then(|p| p.get("driveSize")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("places").and_then(|p| p.get("trashCount")).and_then(Json::as_bool), Some(false));
        // Sidebar040: Recent ships off, so a fresh rail names no recent file.
        assert_eq!(d.get("places").and_then(|p| p.get("showRecent")).and_then(Json::as_bool), Some(false));
        // Decided in 0.3.3: unmounted drives are on the rail unless the operator switches them off.
        assert_eq!(d.get("places").and_then(|p| p.get("showUnmounted")).and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("preview").and_then(|p| p.get("loadOn")).and_then(Json::as_str), Some("automatic"));
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbnails")).and_then(Json::as_str), Some("media"));
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbSize")).and_then(Json::as_str), Some("medium"));
        // ExtThumbs, decided 2026-09-23: network shares and phones are off, USB drives are on.
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbNetwork")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbPhone")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbUsb")).and_then(Json::as_bool), Some(true));
        // Document view flips belong only to an open Quick Look.
        for leaf in ["markdownView", "htmlView"] {
            assert!(d.get("preview").expect("preview").get(leaf).is_none(), "{leaf} is not stored");
        }
        // TallPreviews: the tall kinds show one compact facts line until `i` opens the full table.
        assert_eq!(d.get("previewFacts").and_then(Json::as_str), Some("compact"));
        assert_eq!(d.get("preview").and_then(|p| p.get("wrap")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("display").and_then(|p| p.get("textSize")).and_then(|t| t.get("mode")).and_then(Json::as_str), Some("system"));
        let display: Vec<&str> = d.get("display").and_then(Json::as_object).expect("display").iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(display, ["textSize", "hyprlandIcons"], "the compositor owns opacity, icons and shadows");
        let menu: Vec<&str> = d.get("menu").and_then(Json::as_object).expect("menu").iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(menu, ["hidden"], "the master row is derived from menu.hidden, not stored beside it");
        assert_eq!(d.get("updates").and_then(|u| u.get("autoCheck")).and_then(Json::as_bool), Some(true));
    }

    // menu.hidden stores what is hidden, so an action added later is visible without a migration.
    #[test]
    fn menu_hidden_holds_the_shipped_ids_and_nothing_else() {
        let d = defaults();
        let hidden: Vec<&str> = d
            .get("menu")
            .and_then(|m| m.get("hidden"))
            .and_then(Json::as_array)
            .expect("menu.hidden")
            .iter()
            .filter_map(Json::as_str)
            .collect();
        assert_eq!(
            hidden,
            // HIG CHANGES keeps Actions on while Run Script and Open in Agent remain off; Properties and Permissions ship shown and the view rows leave the file menu.
            ["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "copyAs", "pasteAs", "invertSelection", "extThumbs", "run", "openAsText", "openAgent", "toggleHidden", "toggleLangMarks"]
        );
    }

    // Code icons default on; a saved false stays off, and invalid values heal to the default.
    #[test]
    fn language_marks_default_on_and_keep_a_saved_false() {
        let current = crate::uistate::from_file("{}");
        assert_eq!(current.get("langMarks").and_then(Json::as_bool), Some(true));
        for value in ["true", "false"] {
            let patch = jsondoc::parse(&format!(r#"{{"langMarks":{value}}}"#)).expect("boolean patch");
            let next = crate::uistate::patched(&current, &patch).expect("boolean accepted");
            assert_eq!(next.get("langMarks").and_then(Json::as_bool), Some(value == "true"));
        }
        let off = crate::uistate::from_file(r#"{"langMarks":false}"#);
        let patch = jsondoc::parse(r#"{"hidden":true}"#).expect("unrelated patch");
        let kept = crate::uistate::patched(&off, &patch).expect("saved false retained");
        assert_eq!(kept.get("langMarks").and_then(Json::as_bool), Some(false));
        for value in [r#""true""#, r#""false""#, "1", "null", "[]", "{}"] {
            let text = format!(r#"{{"langMarks":{value}}}"#);
            let patch = jsondoc::parse(&text).expect("patch");
            assert!(crate::uistate::patched(&current, &patch).expect_err("not a boolean").contains("langMarks"));
            assert_eq!(crate::uistate::from_file(&text).get("langMarks").and_then(Json::as_bool), Some(true));
        }
    }

    // The facts mode takes its two words, and a file holding any other word reads as compact.
    #[test]
    fn the_preview_facts_mode_takes_two_words_and_a_bad_one_reads_compact() {
        let current = crate::uistate::from_file("{}");
        let takes = |patch: &str| crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"));
        assert!(takes(r#"{"previewFacts":"compact"}"#).is_ok() && takes(r#"{"previewFacts":"full"}"#).is_ok());
        assert!(takes(r#"{"previewFacts":"wide"}"#).expect_err("refused").contains("previewFacts"));
        let word = |text: &str| crate::uistate::from_file(text).get("previewFacts").and_then(Json::as_str).map(str::to_owned);
        assert_eq!(word(r#"{"previewFacts":"wide"}"#).as_deref(), Some("compact"));
        assert_eq!(word(r#"{"previewFacts":"full"}"#).as_deref(), Some("full"));
    }

    // Each key a place, each value a sort order; past the cap the oldest entries go.
    #[test]
    fn folder_sorts_hold_places_to_orders_and_heal_past_the_cap() {
        let current = crate::uistate::from_file("{}");
        let takes = |patch: &str| crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"));
        for good in [r#"{"folderSorts":{}}"#,
                     r#"{"folderSorts":{"/home/gm/Work":{"key":"size","reverse":true}}}"#,
                     r#"{"folderSorts":{"smb://nas/isos":{"key":"date","reverse":false}}}"#,
                     r#"{"rememberSort":false}"#, r#"{"hiddenLast":true}"#, r#"{"highlightToday":true}"#] {
            assert!(takes(good).is_ok(), "{} is a value its key takes", good);
        }
        for (bad, named) in [(r#"{"folderSorts":[]}"#, "folderSorts"),
                             (r#"{"folderSorts":{"Work":{"key":"size","reverse":true}}}"#, "folderSorts"),
                             (r#"{"folderSorts":{"/a":{"key":"mode","reverse":false}}}"#, "folderSorts"),
                             (r#"{"folderSorts":{"/a":{"key":"size"}}}"#, "folderSorts"),
                             (r#"{"folderSorts":{"/a":{"key":"size","reverse":1}}}"#, "folderSorts"),
                             (r#"{"folderSorts":{"/a":{"key":"size","reverse":false,"by":"x"}}}"#, "folderSorts"),
                             (r#"{"hiddenLast":"yes"}"#, "hiddenLast"),
                             (r#"{"highlightToday":"yes"}"#, "highlightToday"),
                             (r#"{"rememberSort":1}"#, "rememberSort")] {
            let message = takes(bad).expect_err("the patch must be refused");
            assert!(message.contains(named), "{} should name {}, got {}", bad, named, message);
        }
        // A file carrying a bad map costs that key its default and nothing else.
        let read = crate::uistate::from_file(r#"{"folderSorts":{"Work":{"key":"size","reverse":true}},"hiddenLast":true}"#);
        assert_eq!(read.get("folderSorts").and_then(Json::as_object).map(<[(String, Json)]>::len), Some(0));
        assert_eq!(read.get("hiddenLast").and_then(Json::as_bool), Some(true));
        // Past the cap the oldest entries go and the newest stay.
        let mut big = String::from(r#"{"folderSorts":{"#);
        for i in 0..MAX_FOLDER_SORTS + 2 {
            if i > 0 {
                big.push(',');
            }
            big.push_str(&format!(r#""/d{:03}":{{"key":"size","reverse":false}}"#, i));
        }
        big.push_str("}}");
        let kept = takes(&big).expect("a long map still patches");
        let map = kept.get("folderSorts").and_then(Json::as_object).expect("folderSorts");
        assert_eq!(map.len(), MAX_FOLDER_SORTS);
        assert!(map.iter().all(|(k, _)| k != "/d000" && k != "/d001"), "the two oldest go");
        assert!(map.iter().any(|(k, _)| k == "/d501"), "the newest stays");
    }

    // Folder views are opt-in and only name layouts one pane can draw. Dual is a window layout.
    #[test]
    fn folder_views_hold_places_to_browser_modes_and_heal_past_the_cap() {
        let current = crate::uistate::from_file("{}");
        let takes = |patch: &str| crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"));
        for good in [r#"{"rememberViewPerFolder":true}"#,
                     r#"{"folderViews":{"/home/gm/Pictures":"grid","smb://nas/docs":"columns"}}"#] {
            assert!(takes(good).is_ok(), "{} is a value its key takes", good);
        }
        for (bad, named) in [(r#"{"folderViews":[]}"#, "folderViews"),
                             (r#"{"folderViews":{"Pictures":"grid"}}"#, "folderViews"),
                             (r#"{"folderViews":{"/a":"dual"}}"#, "folderViews"),
                             (r#"{"folderViews":{"/a":"tiles"}}"#, "folderViews"),
                             (r#"{"rememberViewPerFolder":1}"#, "rememberViewPerFolder")] {
            let message = takes(bad).expect_err("the patch must be refused");
            assert!(message.contains(named), "{} should name {}, got {}", bad, named, message);
        }
        let read = crate::uistate::from_file(r#"{"folderViews":{"/home/gm/Pictures":"grid","Pictures":"list","/home/gm/Docs":"dual"},"hidden":true}"#);
        let map = read.get("folderViews").and_then(Json::as_object).expect("folderViews");
        assert_eq!(map.len(), 1, "only bad entries go");
        assert_eq!(map.iter().next().map(|(key, value)| (key.as_str(), value.as_str())), Some(("/home/gm/Pictures", Some("grid"))));
        assert_eq!(read.get("hidden").and_then(Json::as_bool), Some(true));
        let mut big = String::from(r#"{"folderViews":{"#);
        for i in 0..MAX_FOLDER_VIEWS + 2 {
            if i > 0 { big.push(','); }
            big.push_str(&format!(r#""/d{:03}":"grid""#, i));
        }
        big.push_str("}}");
        let kept = takes(&big).expect("a long map still patches");
        let map = kept.get("folderViews").and_then(Json::as_object).expect("folderViews");
        assert_eq!(map.len(), MAX_FOLDER_VIEWS);
        assert!(map.iter().all(|(key, _)| key != "/d000" && key != "/d001"));
        assert!(map.iter().any(|(key, _)| key == "/d501"));
    }

    // The rules nothing else reached: an exact stop, a non-empty path, and the four shipped presets.
    #[test]
    fn the_stop_the_preset_and_the_favourites_rules_each_bite_at_their_own_edge() {
        let current = crate::uistate::from_file("{}");
        let takes = |patch: &str| crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"));
        for good in [r#"{"display":{"textSize":{"mode":"system"}}}"#, r#"{"display":{"textSize":{"mode":9}}}"#,
                     r#"{"keys":"default"}"#, r#"{"keys":"vim"}"#,
                     r#"{"keys":"mac"}"#, r#"{"keys":"windows"}"#,
                     r#"{"pickerView":"list"}"#, r#"{"pickerView":"grid"}"#,
                     r#"{"escapeUp":true}"#, r#"{"escapeUp":false}"#,
                     r#"{"openMode":"double"}"#, r#"{"openMode":"single"}"#,
                     r#"{"clickRename":true}"#, r#"{"clickRename":false}"#,
                     r#"{"places":{"favourites":[]}}"#,
                     r#"{"places":{"driveSize":true,"trashCount":true}}"#,
                     r#"{"places":{"driveSize":false,"trashCount":false}}"#,
                     r#"{"places":{"rail":"hidden"}}"#, r#"{"places":{"rail":"shown"}}"#,
                     r#"{"places":{"showUnmounted":false}}"#,
                     r#"{"updates":{"autoCheck":false}}"#] {
            assert!(takes(good).is_ok(), "{} is a value its key takes", good);
        }
        for (bad, named) in [(r#"{"display":{"textSize":{"mode":13}}}"#, "display.textSize.mode"),
                             (r#"{"display":{"textSize":{"mode":14.5}}}"#, "display.textSize.mode"),
                             (r#"{"display":{"textSize":{"mode":"14"}}}"#, "display.textSize.mode"),
                             (r#"{"display":{"textSize":{"mode":"override"}}}"#, "display.textSize.mode"),
                             (r#"{"display":{"opacity":1.0}}"#, "display.opacity"),
                             (r#"{"display":{"shadows":true}}"#, "display.shadows"),
                             (r#"{"menu":{"basic":false}}"#, "menu.basic"),
                             (r#"{"escapeUp":"yes"}"#, "escapeUp"),
                             (r#"{"openMode":"triple"}"#, "openMode"),
                             (r#"{"clickRename":1}"#, "clickRename"),
                             (r#"{"keys":"emacs"}"#, "keys"),
                             (r#"{"pickerView":"columns"}"#, "pickerView"),
                             (r#"{"language":"en"}"#, "language"),
                             (r#"{"places":{"favourites":"/a"}}"#, "places.favourites"),
                             (r#"{"places":{"driveSize":1}}"#, "places.driveSize"),
                             (r#"{"places":{"trashCount":"true"}}"#, "places.trashCount"),
                             (r#"{"places":{"rail":"off"}}"#, "places.rail"),
                             (r#"{"places":{"rail":true}}"#, "places.rail"),
                             (r#"{"updates":{"autoCheck":"yes"}}"#, "updates.autoCheck"),
                             (r#"{"stateVersion":0}"#, "stateVersion"),
                             (r#"{"stateVersion":2}"#, "stateVersion")] {
            let message = takes(bad).expect_err("the patch must be refused");
            assert!(message.contains(named), "{} should name {}, got {}", bad, named, message);
        }
    }

    // Tabs040 callout 2: a bounded list of places in tab order, with the current tab's index
    // inside it. A file carrying a bad one costs that key its default, the way a bad place does.
    #[test]
    fn last_tabs_hold_every_tab_folder_with_its_current_index() {
        let current = crate::uistate::from_file("{}");
        let takes = |patch: &str| crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"));
        for good in [r#"{"lastTabs":{"paths":[],"index":0}}"#,
                     r#"{"lastTabs":{"paths":["/home/gm/Work"],"index":0}}"#,
                     r#"{"lastTabs":{"paths":["/a","smb://nas/isos","/b"],"index":2}}"#] {
            assert!(takes(good).is_ok(), "{} is a value its key takes", good);
        }
        for (bad, named) in [(r#"{"lastTabs":[]}"#, "lastTabs"),
                             (r#"{"lastTabs":{"paths":[],"index":1}}"#, "lastTabs"),
                             (r#"{"lastTabs":{"paths":["/a"],"index":1}}"#, "lastTabs"),
                             (r#"{"lastTabs":{"paths":["/a"],"index":-1}}"#, "lastTabs"),
                             (r#"{"lastTabs":{"paths":["/a"],"index":0.5}}"#, "lastTabs"),
                             (r#"{"lastTabs":{"paths":["Work"],"index":0}}"#, "lastTabs"),
                             (r#"{"lastTabs":{"paths":[""],"index":0}}"#, "lastTabs"),
                             (r#"{"lastTabs":{"paths":["/a"],"index":0,"by":"x"}}"#, "lastTabs"),
                             (r#"{"lastTabs":{"paths":"/a","index":0}}"#, "lastTabs")] {
            let message = takes(bad).expect_err("the patch must be refused");
            assert!(message.contains(named), "{} should name {}, got {}", bad, named, message);
        }
        let mut big = String::from(r#"{"lastTabs":{"paths":["#);
        for i in 0..MAX_LAST_TABS + 1 {
            if i > 0 {
                big.push(',');
            }
            big.push_str(&format!(r#""/d{:03}""#, i));
        }
        big.push_str(r#"],"index":0}}"#);
        let message = takes(&big).expect_err("past the tab cap the patch must be refused");
        assert!(message.contains("lastTabs"), "got {}", message);
        let read = crate::uistate::from_file(r#"{"lastTabs":{"paths":["/a"],"index":3}}"#);
        assert_eq!(read.get("lastTabs").and_then(|t| t.get("paths")).and_then(Json::as_array).map(<[Json]>::len), Some(0));
    }

    #[test]
    fn every_default_key_carries_a_rule_and_no_rule_is_orphaned() {
        assert_eq!(rule_names(SCHEMA), default_names(), "the schema and the shipped shape must name the same keys");
    }
}
