// The ui.json merges with no disk in them: read a file onto the defaults, apply one caller patch,
// and carry 0.1.3's view.json across.
use crate::jsondoc::{self, Json};
use crate::uischema::{defaults, Rule, OPTIONAL_COLUMNS, SCHEMA, TEXT_SIZE_STOPS};

// Never fails: a file this cannot read is a file whose every key falls back to the shipped default.
pub fn from_file(text: &str) -> Json {
    match jsondoc::parse(text) {
        Ok(found) => merge(&defaults(), &found, SCHEMA),
        Err(_) => defaults(),
    }
}

// 0.1.3's $XDG_CONFIG_HOME/flea/view.json. hiddenCols named what was hidden, so this inverts it;
// uiScale is dropped on the operator's ruling, because the new design stores an Omarchy stop only.
pub fn from_view_json(text: &str) -> Json {
    let mut out = defaults();
    let hidden: Vec<String> = match jsondoc::parse(text).ok().as_ref().and_then(|v| v.get("hiddenCols")).and_then(Json::as_array) {
        Some(items) => items.iter().filter_map(Json::as_str).map(str::to_string).collect(),
        None => return out,
    };
    let mut shown = vec![Json::Str("name".to_string())];
    for key in OPTIONAL_COLUMNS {
        if !hidden.iter().any(|h| h == key) {
            shown.push(Json::Str(key.to_string()));
        }
    }
    if let Json::Obj(pairs) = &mut out {
        for pair in pairs.iter_mut() {
            if pair.0 == "columns" {
                pair.1 = Json::Arr(shown);
                break;
            }
        }
    }
    out
}

// The caller-key merge. The whole patch is checked before any of it lands, so a caller that sends
// one bad key changes nothing rather than half of what it asked for.
pub fn patched(current: &Json, patch: &Json) -> Result<Json, String> {
    let pairs = patch.as_object().ok_or_else(|| "a ui.json patch must be a JSON object".to_string())?;
    check(pairs, SCHEMA, "")?;
    // Onto the full shape, never onto whatever the caller happened to hold: a nested patch merges
    // into the object beside it, and an absent one would take the patch's half as the whole key.
    Ok(apply(&merge(&defaults(), current, SCHEMA), patch, SCHEMA))
}

fn check(pairs: &[(String, Json)], schema: &[(&str, Rule)], prefix: &str) -> Result<(), String> {
    for (key, value) in pairs {
        let rule = schema
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, r)| r)
            .ok_or_else(|| format!("{}{} is not a ui.json key", prefix, key))?;
        match rule {
            Rule::Group(sub) => {
                let inner = value
                    .as_object()
                    .ok_or_else(|| format!("{}{} takes an object, not {}", prefix, key, one_line(value)))?;
                check(inner, sub, &format!("{}{}.", prefix, key))?;
            }
            _ if fits(rule, value) => {}
            _ => return Err(format!("{}{} does not take {}", prefix, key, one_line(value))),
        }
    }
    Ok(())
}

fn apply(current: &Json, patch: &Json, schema: &[(&str, Rule)]) -> Json {
    let mut out: Vec<(String, Json)> = current.as_object().map(<[(String, Json)]>::to_vec).unwrap_or_default();
    for (key, value) in patch.as_object().unwrap_or(&[]) {
        let group = schema.iter().find(|(k, _)| k == key).and_then(|(_, r)| match r {
            Rule::Group(sub) => Some(*sub),
            _ => None,
        });
        let held = out.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let next = match (group, held) {
            (Some(sub), Some(existing)) => apply(&existing, value, sub),
            _ => value.clone(),
        };
        match out.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = next,
            None => out.push((key.clone(), next)),
        }
    }
    Json::Obj(out)
}

// Known keys first in the shipped order, then whatever a newer Flea left behind, kept as it was read.
fn merge(default: &Json, found: &Json, schema: &[(&str, Rule)]) -> Json {
    let found_pairs = match found.as_object() {
        Some(pairs) => pairs,
        None => return default.clone(),
    };
    let mut out: Vec<(String, Json)> = Vec::new();
    for (key, rule) in schema {
        let fallback = default.get(key).cloned().unwrap_or(Json::Null);
        let kept = match found.get(key) {
            None => fallback,
            Some(value) => match rule {
                Rule::Group(sub) => merge(&fallback, value, sub),
                _ if fits(rule, value) => value.clone(),
                _ => fallback,
            },
        };
        out.push(((*key).to_string(), kept));
    }
    for (key, value) in found_pairs {
        if !schema.iter().any(|(k, _)| k == key) {
            out.push((key.clone(), value.clone()));
        }
    }
    Json::Obj(out)
}

fn fits(rule: &Rule, value: &Json) -> bool {
    match rule {
        Rule::Bool => value.as_bool().is_some(),
        Rule::Word(words) => value.as_str().map(|s| words.contains(&s)).unwrap_or(false),
        Rule::Words(words) => every_string(value, |s| words.contains(&s)),
        Rule::Paths => every_string(value, |s| !s.is_empty()),
        // Handoff 5a stores the two panes or nothing, so a third path is a shape no restore can read.
        Rule::Pair => match value.as_array() {
            Some(items) => (items.is_empty() || items.len() == 2) && every_string(value, is_a_place),
            None => false,
        },
        Rule::Ids => every_string(value, is_action_id),
        Rule::Count(low, high) => match value.as_f64() {
            Some(n) => n.fract() == 0.0 && n >= *low && n <= *high,
            None => false,
        },
        Rule::Fraction => value.as_f64().map(|n| n > 0.0 && n <= 1.0).unwrap_or(false),
        Rule::TextSize => match value {
            Json::Str(s) => s == "system",
            _ => value.as_f64().map(|n| TEXT_SIZE_STOPS.contains(&n)).unwrap_or(false),
        },
        Rule::Group(_) => value.as_object().is_some(),
    }
}

fn every_string(value: &Json, ok: impl Fn(&str) -> bool) -> bool {
    match value.as_array() {
        Some(items) => items.iter().all(|item| item.as_str().map(&ok).unwrap_or(false)),
        None => false,
    }
}

// An action id is one of keys.toml's own, and 32 of its 48 are camelCase, so this is the shape of
// an identifier rather than a case: ASCII letters, digits, hyphen and underscore, and nothing else.
fn is_action_id(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// A remembered pane is somewhere the restore can list: an absolute path, or a URI naming its root.
fn is_a_place(s: &str) -> bool {
    s.starts_with('/') || s.contains("://")
}

// One elided line for an error sentence, never the whole pretty document. Counted in characters,
// because a value here can be any string a user typed and a byte cut lands inside one.
const ELIDE_AT: usize = 40;
fn one_line(value: &Json) -> String {
    let rendered = jsondoc::render(value);
    let flat: String = rendered.split_whitespace().collect::<Vec<&str>>().join(" ");
    if flat.chars().count() > ELIDE_AT {
        return format!("{}...", flat.chars().take(ELIDE_AT).collect::<String>());
    }
    flat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsondoc;

    fn text(v: &Json) -> String {
        jsondoc::render(v)
    }


    #[test]
    fn a_malformed_file_returns_the_full_default_shape_rather_than_throwing() {
        for bad in ["{oops", "", "[]", "\"a string\"", "{\"view\": }"] {
            assert_eq!(text(&from_file(bad)), text(&defaults()), "{} should read as the defaults", bad);
        }
    }

    #[test]
    fn an_unknown_value_costs_exactly_one_key_and_leaves_every_other_alone() {
        let merged = from_file(r#"{"view":"miller","density":"compact","hidden":true}"#);
        assert_eq!(merged.get("view").and_then(Json::as_str), Some("list"));
        assert_eq!(merged.get("density").and_then(Json::as_str), Some("compact"));
        assert_eq!(merged.get("hidden").and_then(Json::as_bool), Some(true));
    }

    #[test]
    fn a_bad_nested_value_costs_its_own_leaf_and_not_its_siblings() {
        let merged = from_file(r#"{"places":{"showHome":"yes","showTrash":false,"sidebarWidth":0}}"#);
        let places = merged.get("places").expect("places");
        assert_eq!(places.get("showHome").and_then(Json::as_bool), Some(true));
        assert_eq!(places.get("showTrash").and_then(Json::as_bool), Some(false));
        assert_eq!(places.get("sidebarWidth").and_then(Json::as_f64), Some(192.0));
    }

    #[test]
    fn an_unknown_key_is_kept_and_rewritten_untouched_at_both_levels() {
        let merged = from_file(r#"{"fromANewerFlea":{"a":[1,"two"]},"places":{"newLeaf":7}}"#);
        assert_eq!(
            text(merged.get("fromANewerFlea").expect("top-level unknown")),
            "{\n  \"a\": [\n    1,\n    \"two\"\n  ]\n}\n"
        );
        assert_eq!(merged.get("places").and_then(|p| p.get("newLeaf")).and_then(Json::as_f64), Some(7.0));
    }

    #[test]
    fn a_patch_merges_one_nested_leaf_without_clobbering_its_siblings() {
        // Onto a half a document, so the merge cannot be relying on the caller having a full one.
        let half = jsondoc::parse(r#"{"view":"grid"}"#).expect("half");
        let filled = patched(&half, &jsondoc::parse(r#"{"places":{"showHome":false}}"#).expect("patch")).expect("patch applies");
        assert_eq!(filled.get("places").and_then(|p| p.get("showTrash")).and_then(Json::as_bool), Some(true));
        assert_eq!(filled.get("places").and_then(|p| p.get("showHome")).and_then(Json::as_bool), Some(false));
        assert_eq!(filled.get("view").and_then(Json::as_str), Some("grid"));

        let current = from_file("{}");
        let patch = jsondoc::parse(r#"{"sort":{"reverse":true}}"#).expect("patch");
        let next = patched(&current, &patch).expect("patch applies");
        assert_eq!(next.get("sort").and_then(|s| s.get("reverse")).and_then(Json::as_bool), Some(true));
        assert_eq!(next.get("sort").and_then(|s| s.get("key")).and_then(Json::as_str), Some("name"));
    }

    // The TUI submits view, hidden and sort; scale, menus and places are the GUI's and it leaves them alone.
    #[test]
    fn a_tui_patch_and_a_gui_patch_do_not_clobber_each_others_keys() {
        let gui = jsondoc::parse(r#"{"places":{"sidebarWidth":240},"menu":{"basic":false}}"#).expect("gui");
        let tui = jsondoc::parse(r#"{"view":"columns","hidden":true,"sort":{"key":"size"}}"#).expect("tui");
        let after_gui = patched(&from_file("{}"), &gui).expect("gui patch");
        let after_both = patched(&after_gui, &tui).expect("tui patch");
        assert_eq!(after_both.get("places").and_then(|p| p.get("sidebarWidth")).and_then(Json::as_f64), Some(240.0));
        assert_eq!(after_both.get("menu").and_then(|m| m.get("basic")).and_then(Json::as_bool), Some(false));
        assert_eq!(after_both.get("view").and_then(Json::as_str), Some("columns"));
        assert_eq!(after_both.get("hidden").and_then(Json::as_bool), Some(true));
        assert_eq!(after_both.get("sort").and_then(|s| s.get("key")).and_then(Json::as_str), Some("size"));
        let again = patched(&after_both, &gui).expect("gui patch again");
        assert_eq!(again.get("view").and_then(Json::as_str), Some("columns"));
        assert_eq!(again.get("sort").and_then(|s| s.get("key")).and_then(Json::as_str), Some("size"));
    }

    #[test]
    fn a_patch_is_refused_whole_when_a_value_or_a_key_is_not_one_this_flea_knows() {
        let current = from_file("{}");
        for (patch, named) in [
            (r#"{"view":"miller"}"#, "view"),
            (r#"{"places":{"sidebarWidth":"wide"}}"#, "places.sidebarWidth"),
            (r#"{"notAKey":1}"#, "notAKey"),
            (r#"{"menu":{"nope":1}}"#, "menu.nope"),
        ] {
            let p = jsondoc::parse(patch).expect("patch parses");
            let message = patched(&current, &p).expect_err("the patch must be refused");
            assert!(message.contains(named), "{} should name {}, got {}", patch, named, message);
        }
        assert!(patched(&current, &Json::Str("nope".to_string())).is_err());
    }

    // Handoff section 5a: "dual": { "paths": [left, right], "focus": 0 }, and an empty array means
    // no dual-pane locations have been remembered. Three paths is a shape the restore cannot read.
    #[test]
    fn dual_paths_is_two_places_or_none_and_never_a_relative_name() {
        let current = from_file("{}");
        for good in [r#"{"dual":{"paths":[]}}"#, r#"{"dual":{"paths":["/home/gm","/tmp"]}}"#,
                     r#"{"dual":{"paths":["smb://nas/share","/run/user/1000/gvfs/x"]}}"#] {
            let p = jsondoc::parse(good).expect("patch parses");
            assert!(patched(&current, &p).is_ok(), "{} is the shape 5a specifies", good);
        }
        for bad in [r#"{"dual":{"paths":["..","x","y"]}}"#, r#"{"dual":{"paths":["/home/gm"]}}"#,
                    r#"{"dual":{"paths":["..","x"]}}"#, r#"{"dual":{"paths":["/home/gm",""]}}"#] {
            let p = jsondoc::parse(bad).expect("patch parses");
            let message = patched(&current, &p).expect_err("the patch must be refused");
            assert!(message.contains("dual.paths"), "{} should name dual.paths, got {}", bad, message);
        }
        // A file carrying the wrong shape costs that key alone and the pair beside it still stands.
        let read = from_file(r#"{"dual":{"paths":["..","x","y"],"focus":1}}"#);
        assert_eq!(read.get("dual").and_then(|d| d.get("paths")).and_then(Json::as_array).map(<[Json]>::len), Some(0));
        assert_eq!(read.get("dual").and_then(|d| d.get("focus")).and_then(Json::as_f64), Some(1.0));
        // places.favourites keeps the loose rule, because handoff section 4 says a gvfs URI is one.
        let fav = jsondoc::parse(r#"{"places":{"favourites":["/a","/b","/c"]}}"#).expect("patch parses");
        assert!(patched(&current, &fav).is_ok(), "a favourites list is any length");
    }

    // keys.toml holds 48 unique action ids and 32 of them are camelCase, so a hideable row named
    // like one of those has to survive a read rather than take the whole array down with it.
    #[test]
    fn a_camel_case_action_id_is_a_menu_row_this_flea_can_keep_hidden() {
        let merged = from_file(r#"{"menu":{"hidden":["delete","newFolder","copy-path","copy_path"]}}"#);
        let hidden: Vec<&str> = merged
            .get("menu").and_then(|m| m.get("hidden")).and_then(Json::as_array).expect("menu.hidden")
            .iter().filter_map(Json::as_str).collect();
        assert_eq!(hidden, ["delete", "newFolder", "copy-path", "copy_path"]);
        // Still bounded: anything that is not an id costs the key its own default, as it always did.
        for bad in [r#"{"menu":{"hidden":["delete","rm -rf /"]}}"#, r#"{"menu":{"hidden":["delete",""]}}"#,
                    r#"{"menu":{"hidden":["delete","a/b"]}}"#, r#"{"menu":{"hidden":["delete",1]}}"#] {
            let read = from_file(bad);
            let fell_back = read
                .get("menu").and_then(|m| m.get("hidden")).and_then(Json::as_array).expect("menu.hidden");
            assert_eq!(fell_back.len(), 8, "{} must cost the key its own default", bad);
        }
    }

    // hiddenCols named what was hidden; columns names what is shown, so the migration inverts it.
    #[test]
    fn the_view_json_migration_carries_hiddencols_across_and_drops_uiscale() {
        let migrated = from_view_json(r#"{"hiddenCols":["kind","mode"],"uiScale":1.4}"#);
        let cols: Vec<&str> = migrated.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(cols, ["name", "size", "date"]);
        assert!(migrated.get("uiScale").is_none(), "uiScale is dropped, not carried");
        assert_eq!(text(&from_view_json(r#"{"hiddenCols":[]}"#)).contains("\"kind\""), true);
        let nothing_hidden = from_view_json(r#"{"hiddenCols":[]}"#);
        let all: Vec<&str> = nothing_hidden
            .get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(all, ["name", "mode", "size", "date", "kind"]);
        assert_eq!(text(&from_view_json("{oops")), text(&defaults()));
    }
}
