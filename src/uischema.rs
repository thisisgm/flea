// The shipped ui.json shape and the rule each key is measured against; src/uistate.rs applies them.
use crate::jsondoc::{self, Json};

// docs/flea-0.1.4-build-handoff.md section 1's shape and defaults, but density compact (0.3.2), showUnmounted on (0.3.3).
pub const DEFAULTS: &str = r#"{
  "view": "list",
  "density": "compact",
  "columns": ["name", "size", "date"],
  "addressBar": "breadcrumb",
  "sort": { "key": "name", "reverse": false },
  "dual": { "paths": [], "focus": 0 },
  "foldersFirst": true,
  "groupByKind": false,
  "hidden": false,
  "wrapAtEnds": false,
  "keyHints": false,
  "startIn": "home",
  "startFolder": "",
  "lastPath": "",
  "newTab": "current",
  "trashAutoEmpty": false,
  "trashSweptOn": 0,
  "places": {
    "favourites": [],
    "showHome": true, "showNetwork": true,
    "showDevices": true, "showTrash": true,
    "driveSize": false, "trashCount": false, "showUnmounted": true, "rail": "shown", "autoHide": false, "sidebarWidth": 192
  },
  "shelf": {
    "enabled": false, "bar": true, "rail": "off",
    "screenshots": true, "recordings": true, "recent": 3
  },
  "preview": {
    "column": true, "loadOn": "automatic",
    "thumbnails": "media", "thumbSize": "medium",
    "ctrlZoom": true,
    "hideFilmstrip": true, "preloadAdjacent": true,
    "mediaControls": true, "infoPanel": false,
    "hideNavbarFullscreen": true, "navbarHideMs": 4000
  },
  "keys": "default",
  "display": { "textSize": { "mode": "system" }, "hyprlandIcons": false },
  "menu": { "hidden": ["delete", "openTerminal", "placeMenu", "runScript",
            "moveto", "copyto", "properties", "permissions", "copypath"] },
  "updates": { "autoCheck": true },
  "stateVersion": 1
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
    SidebarWidth,
    // dual.paths is the pair handoff 5a specifies, or the empty array that means nothing remembered.
    Pair,
    // menu.hidden is deliberately open: a closed list would make this Flea drop an id a newer one hid.
    Ids,
    Count(f64, f64),
    TextSize,
    // Any whole number, so a newer Flea's higher stamp survives this one's write; no patch may set it.
    Version,
    Group(&'static [(&'static str, Rule)]),
}

// The top-level key that records which of src/uimigrate.rs's one-time changes a file has had.
pub const STATE_VERSION: &str = "stateVersion";

pub const COLUMN_KEYS: &[&str] = &["name", "mode", "size", "date", "kind"];

pub const SORT: &[(&str, Rule)] = &[("key", Rule::Word(&["name", "size", "date", "kind"])), ("reverse", Rule::Bool)];

pub const DUAL: &[(&str, Rule)] = &[("paths", Rule::Pair), ("focus", Rule::Count(0.0, 1.0))];

pub const PLACES: &[(&str, Rule)] = &[
    ("favourites", Rule::Favourites),
    ("showHome", Rule::Bool),
    ("showNetwork", Rule::Bool),
    ("showDevices", Rule::Bool),
    ("showTrash", Rule::Bool),
    ("driveSize", Rule::Bool),
    ("trashCount", Rule::Bool),
    ("showUnmounted", Rule::Bool),
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
    ("thumbSize", Rule::Word(&["small", "medium", "large", "xlarge"])),
    ("ctrlZoom", Rule::Bool),
    ("hideFilmstrip", Rule::Bool),
    ("preloadAdjacent", Rule::Bool),
    ("mediaControls", Rule::Bool),
    ("infoPanel", Rule::Bool),
    ("hideNavbarFullscreen", Rule::Bool),
    ("navbarHideMs", Rule::Count(1000.0, 10000.0)),
];

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
    ("density", Rule::Word(&["compact", "normal", "comfortable"])),
    ("columns", Rule::Columns),
    ("addressBar", Rule::Word(&["path", "breadcrumb"])),
    ("sort", Rule::Group(SORT)),
    ("dual", Rule::Group(DUAL)),
    ("foldersFirst", Rule::Bool),
    ("groupByKind", Rule::Bool),
    ("hidden", Rule::Bool),
    ("wrapAtEnds", Rule::Bool),
    // The Menus section's "Show keyboard hints" row: every menu's key column and the empty
    // directory's own tip, off until it is switched on.
    ("keyHints", Rule::Bool),
    // Where a window opens, and where a new tab opens. "folder" reads startFolder, "last" reads
    // lastPath, which ui/WindowBody.qml writes as the pane moves and no panel control ever touches.
    ("startIn", Rule::Word(&["home", "last", "folder"])),
    ("startFolder", Rule::Place),
    ("lastPath", Rule::Place),
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
    // SettingsKeys.html's four-value chooser over ui/js/Keymap.js's shared tables. A stored name
    // this build cannot honour falls back to default, which is also what a fresh ui.json holds.
    ("keys", Rule::Word(&["default", "vim", "mac", "windows"])),
    ("display", Rule::Group(DISPLAY)),
    ("menu", Rule::Group(MENU)),
    ("updates", Rule::Group(UPDATES)),
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
                "view", "density", "columns", "addressBar", "sort", "dual", "foldersFirst",
                "groupByKind", "hidden", "wrapAtEnds", "keyHints", "startIn", "startFolder",
                "lastPath", "newTab", "trashAutoEmpty", "trashSweptOn", "places", "shelf",
                "preview", "keys",
                "display", "menu", "updates", "stateVersion"
            ]
        );
        assert_eq!(d.get(STATE_VERSION).and_then(Json::as_f64), Some(1.0), "a fresh document is already stamped");
        assert_eq!(d.get("view").and_then(Json::as_str), Some("list"));
        assert_eq!(d.get("density").and_then(Json::as_str), Some("compact"));
        assert_eq!(d.get("addressBar").and_then(Json::as_str), Some("breadcrumb"));
        assert_eq!(d.get("keys").and_then(Json::as_str), Some("default"));
        assert_eq!(d.get("foldersFirst").and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("groupByKind").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("hidden").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("wrapAtEnds").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("keyHints").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("startIn").and_then(Json::as_str), Some("home"));
        assert_eq!(d.get("startFolder").and_then(Json::as_str), Some(""));
        assert_eq!(d.get("lastPath").and_then(Json::as_str), Some(""));
        assert_eq!(d.get("newTab").and_then(Json::as_str), Some("current"));
        assert_eq!(d.get("trashAutoEmpty").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("trashSweptOn").and_then(Json::as_f64), Some(0.0));
        let cols: Vec<&str> = d.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(cols, ["name", "size", "date"]);
        assert_eq!(d.get("sort").and_then(|s| s.get("key")).and_then(Json::as_str), Some("name"));
        assert_eq!(d.get("sort").and_then(|s| s.get("reverse")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("dual").and_then(|s| s.get("paths")).and_then(Json::as_array).map(<[Json]>::len), Some(0));
        assert_eq!(d.get("dual").and_then(|s| s.get("focus")).and_then(Json::as_f64), Some(0.0));
        // Directive 38 and GM's B1 ruling: the shelf ships off, and its switch is what installs the plugin.
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
        // GM's 0.3.3 ruling: unmounted drives are on the rail unless the operator switches them off.
        assert_eq!(d.get("places").and_then(|p| p.get("showUnmounted")).and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("preview").and_then(|p| p.get("loadOn")).and_then(Json::as_str), Some("automatic"));
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbnails")).and_then(Json::as_str), Some("media"));
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbSize")).and_then(Json::as_str), Some("medium"));
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
            // Directive 38: every feature this release adds ships with its own id hidden, so a fresh
            // ui.json behaves as 0.2.1 did. placeMenu is the Places rows' own menu.
            ["delete", "openTerminal", "placeMenu", "runScript", "moveto", "copyto", "properties", "permissions", "copypath"]
        );
    }

    // The rules nothing else reached: an exact stop, a non-empty path, and the four shipped presets.
    #[test]
    fn the_stop_the_preset_and_the_favourites_rules_each_bite_at_their_own_edge() {
        let current = crate::uistate::from_file("{}");
        let takes = |patch: &str| crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"));
        for good in [r#"{"display":{"textSize":{"mode":"system"}}}"#, r#"{"display":{"textSize":{"mode":9}}}"#,
                     r#"{"keys":"default"}"#, r#"{"keys":"vim"}"#,
                     r#"{"keys":"mac"}"#, r#"{"keys":"windows"}"#,
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
                             (r#"{"keys":"emacs"}"#, "keys"),
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

    #[test]
    fn every_default_key_carries_a_rule_and_no_rule_is_orphaned() {
        assert_eq!(rule_names(SCHEMA), default_names(), "the schema and the shipped shape must name the same keys");
    }
}
