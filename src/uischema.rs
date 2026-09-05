// The shipped ui.json shape and the rule each key is measured against; src/uistate.rs applies them.
use crate::jsondoc::{self, Json};

// The shape and every default, copied from docs/flea-0.1.4-build-handoff.md section 1.
pub const DEFAULTS: &str = r#"{
  "view": "list",
  "density": "normal",
  "columns": ["name", "size", "date"],
  "addressBar": "breadcrumb",
  "sort": { "key": "name", "reverse": false },
  "dual": { "paths": [], "focus": 0 },
  "foldersFirst": true,
  "groupByKind": false,
  "hidden": false,
  "wrapAtEnds": false,
  "places": {
    "favourites": [],
    "showHome": true, "showNetwork": true,
    "showDevices": true, "showTrash": true,
    "driveSize": true, "sidebarWidth": 192
  },
  "preview": {
    "column": true, "loadOn": "click",
    "thumbnails": "media", "thumbSize": "medium",
    "ctrlZoom": true
  },
  "keys": "default",
  "display": {
    "textSize": { "mode": "system" },
    "hyprlandIcons": false, "opacity": 1.0,
    "shadows": true
  },
  "menu": { "basic": true, "hidden": ["delete", "openwith", "terminal",
            "moveto", "copyto", "properties", "permissions", "copypath"] },
  "language": "en",
  "updates": { "check": true, "channel": "stable" }
}"#;

// The list row's optional columns in the order ui/js/Columns.js lays them out; name is never optional.
pub const OPTIONAL_COLUMNS: [&str; 4] = ["mode", "size", "date", "kind"];

// Omarchy's own textSizeStops, so an override can never land on a size the OEM panel could not produce.
pub const TEXT_SIZE_STOPS: [f64; 7] = [9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 20.0];
// A rail narrower than a mark plus a label is not a rail, and one wider than this is a second pane.
pub const SIDEBAR_MIN: f64 = 120.0;
pub const SIDEBAR_MAX: f64 = 640.0;

// What a value has to be for the key to keep it. A key that fails its rule falls back to its default.
pub enum Rule {
    Bool,
    Word(&'static [&'static str]),
    // columns names what the list row SHOWS, so it holds each column key at most once and name always.
    Columns,
    Paths,
    // dual.paths is the pair handoff 5a specifies, or the empty array that means nothing remembered.
    Pair,
    // menu.hidden is deliberately open: a closed list would make this Flea drop an id a newer one hid.
    Ids,
    Count(f64, f64),
    Fraction,
    TextSize,
    Group(&'static [(&'static str, Rule)]),
}

pub const COLUMN_KEYS: &[&str] = &["name", "mode", "size", "date", "kind"];

pub const SORT: &[(&str, Rule)] = &[("key", Rule::Word(&["name", "size", "date", "kind"])), ("reverse", Rule::Bool)];

pub const DUAL: &[(&str, Rule)] = &[("paths", Rule::Pair), ("focus", Rule::Count(0.0, 1.0))];

pub const PLACES: &[(&str, Rule)] = &[
    ("favourites", Rule::Paths),
    ("showHome", Rule::Bool),
    ("showNetwork", Rule::Bool),
    ("showDevices", Rule::Bool),
    ("showTrash", Rule::Bool),
    ("driveSize", Rule::Bool),
    ("sidebarWidth", Rule::Count(SIDEBAR_MIN, SIDEBAR_MAX)),
];

pub const PREVIEW: &[(&str, Rule)] = &[
    ("column", Rule::Bool),
    ("loadOn", Rule::Word(&["click", "space"])),
    ("thumbnails", Rule::Word(&["off", "images", "media"])),
    ("thumbSize", Rule::Word(&["small", "medium", "large", "xlarge"])),
    ("ctrlZoom", Rule::Bool),
];

// mode is "system" or one stop, so there is nowhere to put a free number; see the handoff's Display row.
pub const TEXT_SIZE: &[(&str, Rule)] = &[("mode", Rule::TextSize)];

pub const DISPLAY: &[(&str, Rule)] = &[
    ("textSize", Rule::Group(TEXT_SIZE)),
    ("hyprlandIcons", Rule::Bool),
    ("opacity", Rule::Fraction),
    ("shadows", Rule::Bool),
];

pub const MENU: &[(&str, Rule)] = &[("basic", Rule::Bool), ("hidden", Rule::Ids)];

pub const UPDATES: &[(&str, Rule)] = &[("check", Rule::Bool), ("channel", Rule::Word(&["stable"]))];

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
    ("places", Rule::Group(PLACES)),
    ("preview", Rule::Group(PREVIEW)),
    ("keys", Rule::Word(&["default", "vim", "mac", "windows"])),
    ("display", Rule::Group(DISPLAY)),
    ("menu", Rule::Group(MENU)),
    // English is the only catalogue that exists, so anything else is a value this Flea cannot honour.
    ("language", Rule::Word(&["en"])),
    ("updates", Rule::Group(UPDATES)),
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
                "groupByKind", "hidden", "wrapAtEnds", "places", "preview", "keys", "display",
                "menu", "language", "updates"
            ]
        );
        assert_eq!(d.get("view").and_then(Json::as_str), Some("list"));
        assert_eq!(d.get("density").and_then(Json::as_str), Some("normal"));
        assert_eq!(d.get("addressBar").and_then(Json::as_str), Some("breadcrumb"));
        assert_eq!(d.get("keys").and_then(Json::as_str), Some("default"));
        assert_eq!(d.get("language").and_then(Json::as_str), Some("en"));
        assert_eq!(d.get("foldersFirst").and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("groupByKind").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("hidden").and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("wrapAtEnds").and_then(Json::as_bool), Some(false));
        let cols: Vec<&str> = d.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(cols, ["name", "size", "date"]);
        assert_eq!(d.get("sort").and_then(|s| s.get("key")).and_then(Json::as_str), Some("name"));
        assert_eq!(d.get("sort").and_then(|s| s.get("reverse")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("dual").and_then(|s| s.get("paths")).and_then(Json::as_array).map(<[Json]>::len), Some(0));
        assert_eq!(d.get("dual").and_then(|s| s.get("focus")).and_then(Json::as_f64), Some(0.0));
        assert_eq!(d.get("places").and_then(|p| p.get("sidebarWidth")).and_then(Json::as_f64), Some(192.0));
        assert_eq!(d.get("preview").and_then(|p| p.get("loadOn")).and_then(Json::as_str), Some("click"));
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbnails")).and_then(Json::as_str), Some("media"));
        assert_eq!(d.get("preview").and_then(|p| p.get("thumbSize")).and_then(Json::as_str), Some("medium"));
        assert_eq!(d.get("display").and_then(|p| p.get("textSize")).and_then(|t| t.get("mode")).and_then(Json::as_str), Some("system"));
        assert_eq!(d.get("display").and_then(|p| p.get("opacity")).and_then(Json::as_f64), Some(1.0));
        assert_eq!(d.get("display").and_then(|p| p.get("hyprlandIcons")).and_then(Json::as_bool), Some(false));
        assert_eq!(d.get("display").and_then(|p| p.get("shadows")).and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("menu").and_then(|m| m.get("basic")).and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("updates").and_then(|u| u.get("check")).and_then(Json::as_bool), Some(true));
        assert_eq!(d.get("updates").and_then(|u| u.get("channel")).and_then(Json::as_str), Some("stable"));
    }

    // menu.hidden stores what is hidden, so an action added later is visible without a migration.
    #[test]
    fn menu_hidden_holds_the_eight_shipped_ids_and_nothing_else() {
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
            ["delete", "openwith", "terminal", "moveto", "copyto", "properties", "permissions", "copypath"]
        );
    }

    // The rules nothing else reached: an exact stop, a fraction of one and never zero, a non-empty path.
    #[test]
    fn the_stop_the_opacity_and_the_favourites_rules_each_bite_at_their_own_edge() {
        let current = crate::uistate::from_file("{}");
        let takes = |patch: &str| crate::uistate::patched(&current, &jsondoc::parse(patch).expect("patch parses"));
        for good in [r#"{"display":{"textSize":{"mode":"system"}}}"#, r#"{"display":{"textSize":{"mode":9}}}"#,
                     r#"{"display":{"opacity":1.0}}"#, r#"{"display":{"opacity":0.5}}"#,
                     r#"{"places":{"favourites":[]}}"#] {
            assert!(takes(good).is_ok(), "{} is a value its key takes", good);
        }
        for (bad, named) in [(r#"{"display":{"textSize":{"mode":13}}}"#, "display.textSize.mode"),
                             (r#"{"display":{"textSize":{"mode":14.5}}}"#, "display.textSize.mode"),
                             (r#"{"display":{"textSize":{"mode":"14"}}}"#, "display.textSize.mode"),
                             (r#"{"display":{"opacity":0}}"#, "display.opacity"),
                             (r#"{"display":{"opacity":1.5}}"#, "display.opacity"),
                             (r#"{"display":{"opacity":"half"}}"#, "display.opacity"),
                             (r#"{"places":{"favourites":[""]}}"#, "places.favourites"),
                             (r#"{"places":{"favourites":"/a"}}"#, "places.favourites")] {
            let message = takes(bad).expect_err("the patch must be refused");
            assert!(message.contains(named), "{} should name {}, got {}", bad, named, message);
        }
    }

    #[test]
    fn every_default_key_carries_a_rule_and_no_rule_is_orphaned() {
        assert_eq!(rule_names(SCHEMA), default_names(), "the schema and the shipped shape must name the same keys");
    }
}
