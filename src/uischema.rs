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
  "highlightHover": false,
  "tabsAsIcons": false,
  "emptyFolderOmi": true,
  "emptyFolderRotate": true,
  "wrapAtEnds": false,
  "gridWrapRows": false,
  "escapeUp": false,
  "openMode": "double",
  "clickRename": true,
  "openFoldersWhileDragging": false,
  "scrollKeepsCursor": false,
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
    "ctrlZoom": true, "volume": 100, "width": 0, "tracks": [], "wrap": false
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

pub const DUAL: &[(&str, Rule)] = &[
    ("paths", Rule::Pair),
    ("focus", Rule::Count(0.0, 1.0)),
    ("split", Rule::Count(0.0, 10000.0)),
];

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

pub const PREVIEW: &[(&str, Rule)] = &[
    ("column", Rule::Bool),
    ("loadOn", Rule::Word(&["automatic", "manual"])),
    ("thumbnails", Rule::Word(&["off", "images", "media"])),
    (
        "thumbSize",
        Rule::Word(&["small", "medium", "large", "xlarge", "huge", "largest"]),
    ),
    // ExtThumbs: one switch per storage class beside the segment they depend on.
    ("thumbNetwork", Rule::Bool),
    ("thumbPhone", Rule::Bool),
    ("thumbUsb", Rule::Bool),
    ("ctrlZoom", Rule::Bool),
    ("volume", Rule::Count(0.0, 100.0)),
    ("width", Rule::Count(0.0, 100.0)),
    // Columns: each column's share of the room left of the preview, left to right, [] for equal columns.
    ("tracks", Rule::Tracks),
    // Quick Look's w: wraps long lines in a code or text file; off until pressed.
    ("wrap", Rule::Bool),
];

// mode is "system" or one stop, so there is nowhere to put a free number; see the handoff's Display row.
pub const TEXT_SIZE: &[(&str, Rule)] = &[("mode", Rule::TextSize)];

// textSize alone. Window opacity, icon theme and shadows are the compositor's, and Flea mirrors it
// rather than carrying a second writable copy of a setting Hyprland already owns.
pub const DISPLAY: &[(&str, Rule)] = &[
    ("textSize", Rule::Group(TEXT_SIZE)),
    ("hyprlandIcons", Rule::Bool),
];

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
    (
        "density",
        Rule::Word(&["tight", "compact", "normal", "comfortable"]),
    ),
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
    ("highlightHover", Rule::Bool),
    // Settings > View's tab marks and the empty folder's two rows; off, on and on draw 0.4.1's window.
    ("tabsAsIcons", Rule::Bool),
    ("emptyFolderOmi", Rule::Bool),
    ("emptyFolderRotate", Rule::Bool),
    ("wrapAtEnds", Rule::Bool),
    ("gridWrapRows", Rule::Bool),
    // ClickAndRefresh: Escape climbs when on, the pointer's open mode, and the
    // slow-click rename it greys. Single click and Escape-up ship off, slow click ships on.
    ("escapeUp", Rule::Bool),
    ("openMode", Rule::Word(&["double", "single"])),
    ("clickRename", Rule::Bool),
    ("openFoldersWhileDragging", Rule::Bool),
    // A wheel, touchpad or scrollbar scroll moves only the viewport and leaves the cursor on its row; off until chosen.
    ("scrollKeepsCursor", Rule::Bool),
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
#[path = "uischema_defaults_tests.rs"]
mod defaults_tests;
#[cfg(test)]
#[path = "uischema_rules_tests.rs"]
mod rules_tests;
