// The ui.json merges with no disk in them: read a file onto the defaults, apply one caller patch,
// and carry 0.1.3's view.json across.
use crate::jsondoc::{self, Json};
use crate::uischema::{defaults, Rule, COLUMN_KEYS, COLUMN_WIDTH_KEYS, COLUMN_WIDTH_MAX, COLUMN_WIDTH_MIN, MAX_LAST_TABS, OPTIONAL_COLUMNS, SCHEMA, TEXT_SIZE_STOPS, SIDEBAR_STOPS, SORT_KEYS, MAX_FOLDER_SORTS, MAX_FOLDER_VIEWS};

// Never fails: a file this cannot read is a file whose every key falls back to the shipped default.
pub fn from_file(text: &str) -> Json {
    match jsondoc::parse(text) {
        Ok(found) => crate::uimigrate::migrated(&found, merge(&defaults(), &found, SCHEMA)),
        Err(_) => defaults(),
    }
}

// 0.1.3's $XDG_CONFIG_HOME/flea/view.json. hiddenCols named what was hidden, so this inverts it;
// uiScale is dropped deliberately, because the new design stores an Omarchy stop only.
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
            // A front end that could write the stamp could rewind a migration the file has already had.
            Rule::Version => return Err(format!("{}{} is kept by Flea and no patch sets it", prefix, key)),
            // A null folderSorts entry forgets that path, and only a patch may send one.
            Rule::FolderSorts if is_folder_sorts_patch(value) => {}
            // A null folderViews entry forgets that path, and only a patch may send one.
            Rule::FolderViews if is_folder_views_patch(value) => {}
            Rule::Plugins if crate::uiplugins::is_patch(value) => {}
            _ if fits(rule, value) => {}
            _ => return Err(format!("{}{} does not take {}", prefix, key, one_line(value))),
        }
    }
    Ok(())
}

fn apply(current: &Json, patch: &Json, schema: &[(&str, Rule)]) -> Json {
    let mut out: Vec<(String, Json)> = current.as_object().map(<[(String, Json)]>::to_vec).unwrap_or_default();
    for (key, value) in patch.as_object().unwrap_or(&[]) {
        let rule = schema.iter().find(|(name, _)| name == key).map(|(_, r)| r);
        let group = rule.and_then(|r| match r {
            Rule::Group(sub) => Some(*sub),
            _ => None,
        });
        let held = out.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let next = match (group, held, rule) {
            (Some(sub), Some(existing), _) => apply(&existing, value, sub),
            // A map patch merges per entry, so one window's write keeps what another wrote since its read.
            (None, held, Some(Rule::FolderSorts)) => merge_folder_sorts(held, value),
            (None, held, Some(Rule::FolderViews)) => merge_folder_views(held, value),
            (None, held, Some(Rule::ColumnWidths)) => merge_column_widths(held, value),
            (None, held, Some(Rule::Plugins)) => crate::uiplugins::merged(held, value),
            _ => normalized(rule, value),
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
                // A hand-edited map heals one bad entry rather than losing the key.
                Rule::FolderSorts => normalized(Some(rule), &clean_folder_sorts(value)),
                Rule::FolderViews => normalized(Some(rule), &clean_folder_views(value)),
                Rule::Plugins => crate::uiplugins::clean(value),
                _ if fits(rule, value) => normalized(Some(rule), value),
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

// A folder patch names only what changed: each path is forgotten, then re-pushed newest.
fn merge_folder_sorts(held: Option<Json>, patch: &Json) -> Json {
    let mut pairs: Vec<(String, Json)> = held.as_ref().and_then(Json::as_object).map(<[(String, Json)]>::to_vec).unwrap_or_default();
    if let Some(entries) = patch.as_object() {
        for (path, order) in entries {
            pairs.retain(|(k, _)| k != path);
            if !matches!(order, Json::Null) {
                pairs.push((path.clone(), order.clone()));
            }
        }
    }
    normalized(Some(&Rule::FolderSorts), &Json::Obj(pairs))
}

// A folder-view patch names only what changed: each path is forgotten, then re-pushed newest.
fn merge_folder_views(held: Option<Json>, patch: &Json) -> Json {
    let mut pairs: Vec<(String, Json)> = held.as_ref().and_then(Json::as_object).map(<[(String, Json)]>::to_vec).unwrap_or_default();
    if let Some(entries) = patch.as_object() {
        for (path, view) in entries {
            pairs.retain(|(k, _)| k != path);
            if !matches!(view, Json::Null) {
                pairs.push((path.clone(), view.clone()));
            }
        }
    }
    normalized(Some(&Rule::FolderViews), &Json::Obj(pairs))
}

// A widths patch replaces only the edges it names, leaving every other edge standing.
fn merge_column_widths(held: Option<Json>, patch: &Json) -> Json {
    let mut pairs: Vec<(String, Json)> = held.as_ref().and_then(Json::as_object).map(<[(String, Json)]>::to_vec).unwrap_or_default();
    if let Some(entries) = patch.as_object() {
        for (key, width) in entries {
            pairs.retain(|(k, _)| k != key);
            pairs.push((key.clone(), width.clone()));
        }
    }
    Json::Obj(pairs)
}

fn normalized(rule: Option<&Rule>, value: &Json) -> Json {
    if matches!(rule, Some(Rule::SidebarWidth)) {
        let requested = value.as_f64().unwrap_or(192.0);
        let mut nearest = SIDEBAR_STOPS[0];
        for stop in SIDEBAR_STOPS {
            if (stop - requested).abs() < (nearest - requested).abs() { nearest = stop; }
        }
        return Json::Num(format!("{}", nearest));
    }
    // The maps are oldest first, so a write past their cap drops the longest-ago entry.
    if matches!(rule, Some(Rule::FolderSorts) | Some(Rule::FolderViews)) {
        if let Json::Obj(pairs) = value {
            let cap = if matches!(rule, Some(Rule::FolderViews)) { MAX_FOLDER_VIEWS } else { MAX_FOLDER_SORTS };
            if pairs.len() > cap {
                return Json::Obj(pairs[pairs.len() - cap..].to_vec());
            }
        }
        return value.clone();
    }
    value.clone()
}

fn fits(rule: &Rule, value: &Json) -> bool {
    match rule {
        Rule::Bool => value.as_bool().is_some(),
        Rule::Word(words) => value.as_str().map(|s| words.contains(&s)).unwrap_or(false),
        Rule::Columns => is_column_set(value),
        Rule::Favourites => value.as_array().is_some(),
        Rule::SidebarWidth => value.as_f64().is_some_and(f64::is_finite),
        // Handoff 5a stores the two panes or nothing, so a third path is a shape no restore can read.
        Rule::Pair => match value.as_array() {
            Some(items) => (items.is_empty() || items.len() == 2) && every_string(value, is_a_place),
            None => false,
        },
        // An absolute path or a uri, or "" for one nothing has chosen or recorded. Nothing else:
        // a relative path in ui.json would be resolved against whatever directory Flea was started in.
        Rule::Place => match value.as_str() {
            Some(text) => text.is_empty() || is_a_place(text),
            None => false,
        },
        Rule::Ids => every_string(value, is_action_id),
        // The whole remembered strip stands or falls together, like lastPath does.
        Rule::LastTabs => is_last_tabs(value),
        Rule::FolderSorts => is_folder_sorts(value),
        Rule::FolderViews => is_folder_views(value),
        Rule::ColumnWidths => is_column_widths(value),
        Rule::Tracks => value.as_array().is_some_and(|items| {
            items.len() <= 4 && items.iter().all(|item| fits(&Rule::Count(1.0, 10000.0), item))
        }),
        Rule::Plugins => crate::uiplugins::fits(value),
        Rule::Count(low, high) => match value.as_f64() {
            Some(n) => n.fract() == 0.0 && n >= *low && n <= *high,
            None => false,
        },
        Rule::Version => fits(&Rule::Count(0.0, f64::MAX), value),
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

// The list row's own set. uischema's columns row says name is never optional and ui/js/Columns.js
// draws it whatever this holds, so every entry is a column key, each of them once, and name is one.
fn is_column_set(value: &Json) -> bool {
    let items = match value.as_array() {
        Some(items) => items,
        None => return false,
    };
    let mut seen: Vec<&str> = Vec::new();
    for item in items {
        match item.as_str() {
            Some(key) if COLUMN_KEYS.contains(&key) && !seen.contains(&key) => seen.push(key),
            _ => return false,
        }
    }
    seen.contains(&"name")
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

// Every open tab's folder in order, with the current tab's index inside the list.
// Sample input: {"paths": ["/a", "/b"], "index": 1}.
fn is_last_tabs(value: &Json) -> bool {
    let pairs = match value.as_object() {
        Some(pairs) => pairs,
        None => return false,
    };
    if pairs.len() != 2 {
        return false;
    }
    let paths = match value.get("paths") {
        Some(Json::Arr(items)) => items,
        _ => return false,
    };
    if paths.len() > MAX_LAST_TABS || !paths.iter().all(|p| p.as_str().map(is_a_place).unwrap_or(false)) {
        return false;
    }
    match value.get("index").and_then(Json::as_f64) {
        Some(n) if n.fract() == 0.0 && n >= 0.0 => {
            let at = n as usize;
            if paths.is_empty() { at == 0 } else { at < paths.len() }
        }
        _ => false,
    }
}

// ListColumns040's remembered widths: each entry a resizable column key to whole pixels.
// Sample input: {"size": 120, "date": 140}.
fn is_column_widths(value: &Json) -> bool {
    match value.as_object() {
        Some(pairs) => pairs.iter().all(|(key, width)| {
            COLUMN_WIDTH_KEYS.contains(&key.as_str())
                && width.as_f64().is_some_and(|n| n.is_finite() && n.fract() == 0.0 && (COLUMN_WIDTH_MIN..=COLUMN_WIDTH_MAX).contains(&n))
        }),
        None => false,
    }
}

// A patch names only what changed, so a null folderSorts entry forgets that path.
fn is_folder_sorts_patch(value: &Json) -> bool {
    match value.as_object() {
        Some(pairs) => pairs.iter().all(|(path, order)| is_a_place(path) && (matches!(order, Json::Null) || is_sort_order(order))),
        None => false,
    }
}

// The per-folder sort map: each key a place, each value a sort order in sort's own shape.
// Sample input: {"/home/gm/Work": {"key": "size", "reverse": true}}.
fn is_folder_sorts(value: &Json) -> bool {
    match value.as_object() {
        Some(pairs) => pairs.iter().all(|(path, order)| is_a_place(path) && is_sort_order(order)),
        None => false,
    }
}

// A read keeps the entries a hand edit left valid and drops only the bad ones.
// Sample input: {"/home/gm/Work": {"key": "size", "reverse": true}}.
fn clean_folder_sorts(value: &Json) -> Json {
    match value.as_object() {
        Some(pairs) => Json::Obj(pairs.iter().filter(|(path, order)| is_a_place(path) && is_sort_order(order)).map(|(path, order)| (path.clone(), order.clone())).collect()),
        None => Json::Obj(Vec::new()),
    }
}

// A patch may forget one path with null; a stored value is one of the three browser views.
fn is_folder_views_patch(value: &Json) -> bool {
    match value.as_object() {
        Some(pairs) => pairs.iter().all(|(path, view)| is_a_place(path) && (matches!(view, Json::Null) || is_folder_view(view))),
        None => false,
    }
}

// The per-folder view map: each path holds a browser view, never the dual-pane window layout.
fn is_folder_views(value: &Json) -> bool {
    match value.as_object() {
        Some(pairs) => pairs.iter().all(|(path, view)| is_a_place(path) && is_folder_view(view)),
        None => false,
    }
}

fn clean_folder_views(value: &Json) -> Json {
    match value.as_object() {
        Some(pairs) => Json::Obj(pairs.iter().filter(|(path, view)| is_a_place(path) && is_folder_view(view)).map(|(path, view)| (path.clone(), view.clone())).collect()),
        None => Json::Obj(Vec::new()),
    }
}

fn is_folder_view(value: &Json) -> bool {
    value.as_str().is_some_and(|view| ["list", "columns", "grid"].contains(&view))
}

// Sample input: {"key": "size", "reverse": true}.
fn is_sort_order(value: &Json) -> bool {
    match value.as_object() {
        Some(pairs) => {
            pairs.len() == 2
                && pairs.iter().any(|(k, v)| k == "key" && v.as_str().is_some_and(|s| SORT_KEYS.contains(&s)))
                && pairs.iter().any(|(k, v)| k == "reverse" && v.as_bool().is_some())
        }
        None => false,
    }
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
    fn favourite_records_survive_and_width_snaps_independently() {
        let text = r#"{"places":{"favourites":[{"label":"","path":"relative"},17,"legacy",{"label":"A","path":"/a"},{"label":"A","path":"/a"}],"sidebarWidth":999}}"#;
        let original = jsondoc::parse(text).unwrap();
        let state = from_file(text);
        assert_eq!(state.get("places").unwrap().get("favourites"), original.get("places").unwrap().get("favourites"));
        assert_eq!(state.get("places").unwrap().get("sidebarWidth").unwrap().as_f64(), Some(256.0));
        let invalid = from_file(r#"{"places":{"sidebarWidth":"wide"}}"#);
        assert_eq!(invalid.get("places").unwrap().get("sidebarWidth").unwrap().as_f64(), Some(192.0));
    }

    #[test]
    fn a_malformed_file_returns_the_full_default_shape_rather_than_throwing() {
        for bad in ["{oops", "", "[]", "\"a string\"", "{\"view\": }"] {
            assert_eq!(text(&from_file(bad)), text(&defaults()), "{} should read as the defaults", bad);
        }
    }

    #[test]
    fn an_unknown_value_costs_exactly_one_key_and_leaves_every_other_alone() {
        let merged = from_file(r#"{"view":"miller","density":"comfortable","hidden":true}"#);
        assert_eq!(merged.get("view").and_then(Json::as_str), Some("list"));
        assert_eq!(merged.get("density").and_then(Json::as_str), Some("comfortable"));
        assert_eq!(merged.get("hidden").and_then(Json::as_bool), Some(true));
        // The picker's own view answers the same way: a word it cannot draw costs the default.
        let remembered = from_file(r#"{"pickerView":"columns"}"#);
        assert_eq!(remembered.get("pickerView").and_then(Json::as_str), Some("list"));
        let kept = from_file(r#"{"pickerView":"grid"}"#);
        assert_eq!(kept.get("pickerView").and_then(Json::as_str), Some("grid"));
    }

    // SettingsKeys.html: a missing or unrecognised preset falls back to Default, discarding nothing.
    #[test]
    fn a_stored_preset_this_build_cannot_honour_reads_back_as_default() {
        assert_eq!(from_file("{}").get("keys").and_then(Json::as_str), Some("default"));
        for stored in ["default", "vim", "mac", "windows"] {
            let kept = from_file(&format!(r#"{{"keys":"{}"}}"#, stored));
            assert_eq!(kept.get("keys").and_then(Json::as_str), Some(stored), "{} is a shipped preset", stored);
        }
        let merged = from_file(r#"{"keys":"emacs","hidden":true}"#);
        assert_eq!(merged.get("keys").and_then(Json::as_str), Some("default"));
        assert_eq!(merged.get("hidden").and_then(Json::as_bool), Some(true), "the bad name costs only its own key");
    }

    #[test]
    fn a_bad_nested_value_costs_its_own_leaf_and_not_its_siblings() {
        let merged = from_file(r#"{"places":{"showHome":"yes","showTrash":false,"sidebarWidth":0}}"#);
        let places = merged.get("places").expect("places");
        assert_eq!(places.get("showHome").and_then(Json::as_bool), Some(true));
        assert_eq!(places.get("showTrash").and_then(Json::as_bool), Some(false));
        assert_eq!(places.get("sidebarWidth").and_then(Json::as_f64), Some(160.0));
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

    // Retired view keys load as unknown leaves, but cannot be patched into state.
    #[test]
    fn retired_document_views_load_but_patches_are_refused() {
        let merged = from_file(r#"{"preview":{"markdownView":"source","htmlView":"source","thumbSize":"large"},"view":"columns"}"#);
        assert_eq!(merged.get("view").and_then(Json::as_str), Some("columns"));
        let preview = merged.get("preview").expect("preview");
        assert_eq!(preview.get("thumbSize").and_then(Json::as_str), Some("large"));
        assert_eq!(preview.get("markdownView").and_then(Json::as_str), Some("source"));
        assert_eq!(preview.get("htmlView").and_then(Json::as_str), Some("source"));
        let next = patched(&merged, &jsondoc::parse(r#"{"density":"tight"}"#).unwrap()).expect("unrelated patch applies");
        let loaded = from_file(&text(&next));
        for leaf in ["markdownView", "htmlView"] {
            assert!(from_file("{}").get("preview").unwrap().get(leaf).is_none(), "fresh state has no {leaf}");
            assert_eq!(loaded.get("preview").unwrap().get(leaf).and_then(Json::as_str), Some("source"));
            for value in ["source", "rendered", "unknown"] {
                let patch = jsondoc::parse(&format!(r#"{{"preview":{{"{leaf}":"{value}"}}}}"#)).unwrap();
                assert!(patched(&from_file("{}"), &patch).unwrap_err().contains(leaf));
                assert!(patched(&loaded, &patch).unwrap_err().contains(leaf));
            }
        }
        let unknown = jsondoc::parse(r#"{"preview":{"futureDocumentView":"source"}}"#).unwrap();
        assert!(patched(&merged, &unknown).unwrap_err().contains("futureDocumentView"));
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
        let gui = jsondoc::parse(r#"{"places":{"sidebarWidth":240},"menu":{"hidden":["paste"]}}"#).expect("gui");
        let tui = jsondoc::parse(r#"{"view":"columns","hidden":true,"sort":{"key":"size"}}"#).expect("tui");
        let after_gui = patched(&from_file("{}"), &gui).expect("gui patch");
        let after_both = patched(&after_gui, &tui).expect("tui patch");
        assert_eq!(after_both.get("places").and_then(|p| p.get("sidebarWidth")).and_then(Json::as_f64), Some(224.0));
        let hidden = after_both.get("menu").and_then(|m| m.get("hidden")).and_then(Json::as_array).expect("menu.hidden");
        assert_eq!(hidden.iter().filter_map(Json::as_str).collect::<Vec<&str>>(), ["paste"]);
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
        let enabled = jsondoc::parse(r#"{"view":"dual","dual":{"paths":["/tmp/left","/tmp/right"],"focus":1}}"#).expect("dual patch");
        let restored = patched(&from_file("{}"), &enabled).expect("dual view is implemented");
        assert_eq!(restored.get("view").and_then(Json::as_str), Some("dual"));
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

    // The Dual divider's remembered share is a whole number of ten-thousandths, 0 to 10000, and 5000 is the equal split.
    #[test]
    fn dual_split_is_a_whole_share_and_a_bad_one_costs_that_leaf_alone() {
        let current = from_file("{}");
        for good in ["0", "5000", "10000"] {
            let p = jsondoc::parse(&format!(r#"{{"dual":{{"split":{}}}}}"#, good)).expect("patch parses");
            let stored = patched(&current, &p).expect("a share in range is stored");
            assert_eq!(stored.get("dual").and_then(|d| d.get("split")).and_then(Json::as_f64), Some(good.parse::<f64>().unwrap()));
        }
        for bad in ["5000.5", "-1", "10001", r#""x""#] {
            let p = jsondoc::parse(&format!(r#"{{"dual":{{"split":{}}}}}"#, bad)).expect("patch parses");
            let message = patched(&current, &p).expect_err("the patch must be refused");
            assert!(message.contains("dual.split"), "{} should name dual.split, got {}", bad, message);
            let read = from_file(&format!(r#"{{"dual":{{"paths":["/tmp/left","/tmp/right"],"focus":1,"split":{}}}}}"#, bad));
            let dual = read.get("dual").expect("dual group");
            assert_eq!(dual.get("split").and_then(Json::as_f64), Some(5000.0), "{} reads back as the equal split", bad);
            assert_eq!(dual.get("focus").and_then(Json::as_f64), Some(1.0));
            assert_eq!(dual.get("paths").and_then(Json::as_array).map(<[Json]>::len), Some(2));
        }
        // The pane's own save is a {focus, paths} patch, and it must leave a stored share alone.
        let held = from_file(r#"{"dual":{"split":6500}}"#);
        let patch = jsondoc::parse(r#"{"dual":{"focus":1,"paths":["/tmp/left","/tmp/right"]}}"#).expect("patch parses");
        let saved = patched(&held, &patch).expect("the pane's save is accepted");
        assert_eq!(saved.get("dual").and_then(|d| d.get("split")).and_then(Json::as_f64), Some(6500.0));
        assert_eq!(saved.get("dual").and_then(|d| d.get("focus")).and_then(Json::as_f64), Some(1.0));
    }

    // keys.toml holds 48 unique action ids and 32 of them are camelCase, so a hideable row named
    // like one of those has to survive a read rather than take the whole array down with it.
    #[test]
    fn a_camel_case_action_id_is_a_menu_row_this_flea_can_keep_hidden() {
        let merged = from_file(r#"{"menu":{"hidden":["delete","newFolder","copy-path","copy_path"]}}"#);
        let hidden: Vec<&str> = merged
            .get("menu").and_then(|m| m.get("hidden")).and_then(Json::as_array).expect("menu.hidden")
            .iter().filter_map(Json::as_str).collect();
        assert_eq!(hidden, ["delete", "newFolder", "copy-path", "copy_path", "pasteAs", "invertSelection", "run", "openAsText", "openAgent"],
            "the ids survive the read, and the migration appends what the file never named");
        // Still bounded: anything that is not an id costs the key its own default, as it always did.
        let shipped = crate::uischema::defaults();
        let shipped = shipped
            .get("menu").and_then(|m| m.get("hidden")).and_then(Json::as_array).expect("shipped menu.hidden").len();
        for bad in [r#"{"menu":{"hidden":["delete","rm -rf /"]}}"#, r#"{"menu":{"hidden":["delete",""]}}"#,
                    r#"{"menu":{"hidden":["delete","a/b"]}}"#, r#"{"menu":{"hidden":["delete",1]}}"#] {
            let read = from_file(bad);
            let fell_back = read
                .get("menu").and_then(|m| m.get("hidden")).and_then(Json::as_array).expect("menu.hidden");
            assert_eq!(fell_back.len(), shipped, "{} must cost the key its own default", bad);
        }
    }

    // Measured through the real singleton: seeded ["name","size","size","date"], one header-menu
    // "Hide Size" click left the drawn set at name,size,date, so a duplicate costs the user a click.
    #[test]
    fn a_columns_array_holds_each_key_once_and_always_name() {
        let current = from_file("{}");
        for good in [r#"{"columns":["name"]}"#, r#"{"columns":["name","size","date"]}"#,
                     r#"{"columns":["kind","name","mode","size","date"]}"#] {
            let p = jsondoc::parse(good).expect("patch parses");
            assert!(patched(&current, &p).is_ok(), "{} is a column set", good);
        }
        for bad in [r#"{"columns":[]}"#, r#"{"columns":["size","date"]}"#, r#"{"columns":["name","name"]}"#,
                    r#"{"columns":["name","size","size"]}"#, r#"{"columns":["name","owner"]}"#,
                    r#"{"columns":"name"}"#, r#"{"columns":["name",1]}"#] {
            let p = jsondoc::parse(bad).expect("patch parses");
            let message = patched(&current, &p).expect_err("the patch must be refused");
            assert!(message.contains("columns"), "{} should name columns, got {}", bad, message);
        }
        // A file carrying one costs that key its own default, and the key beside it still stands.
        let read = from_file(r#"{"columns":["size","size"],"density":"comfortable"}"#);
        let cols: Vec<&str> = read.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(cols, ["name", "size", "date"]);
        assert_eq!(read.get("density").and_then(Json::as_str), Some("comfortable"));
    }

    // ColumnsWidth caps the count at 5 and ListColumns040 remembers a dragged edge per column.
    #[test]
    fn a_column_limit_caps_and_a_width_remembers_only_its_own_rails() {
        let current = from_file("{}");
        for good in [r#"{"columnsLimit":2}"#, r#"{"columnsLimit":5}"#,
                     r#"{"columnWidths":{}}"#,
                     r#"{"columnWidths":{"size":120}}"#,
                     r#"{"columnWidths":{"mode":48,"size":70,"date":480,"kind":130}}"#] {
            let p = jsondoc::parse(good).expect("patch parses");
            assert!(patched(&current, &p).is_ok(), "{} is a column value", good);
        }
        for (bad, named) in [(r#"{"columnsLimit":1}"#, "columnsLimit"),
                             (r#"{"columnsLimit":6}"#, "columnsLimit"),
                             (r#"{"columnsLimit":"5"}"#, "columnsLimit"),
                             (r#"{"columnWidths":{"size":47}}"#, "columnWidths"),
                             (r#"{"columnWidths":{"size":481}}"#, "columnWidths"),
                             (r#"{"columnWidths":{"name":100}}"#, "columnWidths"),
                             (r#"{"columnWidths":[]}"#, "columnWidths")] {
            let p = jsondoc::parse(bad).expect("patch parses");
            let message = patched(&current, &p).expect_err("the patch must be refused");
            assert!(message.contains(named), "{} should name {}, got {}", bad, named, message);
        }
        // A file carrying one costs that key its own default, and the key beside it still stands.
        let read = from_file(r#"{"columnsLimit":9,"columnWidths":{"size":10},"density":"comfortable"}"#);
        assert_eq!(read.get("columnsLimit").and_then(Json::as_f64), Some(3.0));
        assert_eq!(read.get("columnWidths").and_then(Json::as_object).map(<[(String, Json)]>::len), Some(0));
        assert_eq!(read.get("density").and_then(Json::as_str), Some("comfortable"));
    }

    // hiddenCols named what was hidden; columns names what is shown, so the migration inverts it.
    #[test]
    fn the_view_json_migration_carries_hiddencols_across_and_drops_uiscale() {
        let migrated = from_view_json(r#"{"hiddenCols":["kind","mode"],"uiScale":1.4}"#);
        let cols: Vec<&str> = migrated.get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(cols, ["name", "size", "date"]);
        assert!(migrated.get("uiScale").is_none(), "uiScale is dropped, not carried");
        assert!(text(&from_view_json(r#"{"hiddenCols":[]}"#)).contains("\"kind\""));
        let nothing_hidden = from_view_json(r#"{"hiddenCols":[]}"#);
        let all: Vec<&str> = nothing_hidden
            .get("columns").and_then(Json::as_array).expect("columns").iter().filter_map(Json::as_str).collect();
        assert_eq!(all, ["name", "mode", "size", "date", "kind"]);
        assert_eq!(text(&from_view_json("{oops")), text(&defaults()));
    }

    // Settings > View > Opening. A place or "", and nothing else: a relative path in ui.json would be
    // resolved against whatever directory Flea happened to be started in.
    #[test]
    fn an_opening_folder_is_a_place_or_nothing() {
        let kept = from_file(r#"{"startIn":"folder","startFolder":"/home/gm/Work","newTab":"home"}"#);
        assert_eq!(kept.get("startIn").and_then(Json::as_str), Some("folder"));
        assert_eq!(kept.get("startFolder").and_then(Json::as_str), Some("/home/gm/Work"));
        assert_eq!(kept.get("newTab").and_then(Json::as_str), Some("home"));
        let uri = from_file(r#"{"lastPath":"smb://example.com/isos"}"#);
        assert_eq!(uri.get("lastPath").and_then(Json::as_str), Some("smb://example.com/isos"));
        let empty = from_file(r#"{"startFolder":""}"#);
        assert_eq!(empty.get("startFolder").and_then(Json::as_str), Some(""));
        for bad in [
            r#"{"startFolder":"Work"}"#,
            r#"{"startFolder":"../Work"}"#,
            r#"{"startFolder":7}"#,
            r#"{"startFolder":null}"#,
            r#"{"startFolder":["/home/gm"]}"#,
        ] {
            let refused = from_file(bad);
            assert_eq!(refused.get("startFolder").and_then(Json::as_str), Some(""), "refused: {bad}");
        }
        let mode = from_file(r#"{"startIn":"fromANewerFlea","newTab":"elsewhere"}"#);
        assert_eq!(mode.get("startIn").and_then(Json::as_str), Some("home"));
        assert_eq!(mode.get("newTab").and_then(Json::as_str), Some("current"));
    }

    // A width that is not a whole pixel is refused, the comment above is_column_widths.
    #[test]
    fn a_fractional_column_width_is_refused_while_whole_ones_land() {
        let current = from_file("{}");
        let half = jsondoc::parse(r#"{"columnWidths":{"size":120.5}}"#).expect("patch parses");
        let message = patched(&current, &half).expect_err("120.5 px is not a whole pixel");
        assert!(message.contains("columnWidths"), "got {}", message);
        let whole = jsondoc::parse(r#"{"columnWidths":{"size":120,"date":140}}"#).expect("patch parses");
        assert!(patched(&current, &whole).is_ok(), "whole pixels still land");
        let read = from_file(r#"{"columnWidths":{"size":120.5},"density":"comfortable"}"#);
        assert_eq!(read.get("columnWidths").and_then(Json::as_object).map(<[(String, Json)]>::len), Some(0));
        assert_eq!(read.get("density").and_then(Json::as_str), Some("comfortable"));
    }

    // One bad folderSorts entry drops only itself on read; a patch naming one is still refused whole.
    #[test]
    fn a_bad_folder_sort_entry_costs_itself_and_not_the_map() {
        let read = from_file(r#"{"folderSorts":{"/home/gm/Work":{"key":"size","reverse":true},"Work":{"key":"size","reverse":true},"/home/gm/Photos":{"key":"date","reverse":false}}}"#);
        let map = read.get("folderSorts").and_then(Json::as_object).expect("folderSorts");
        assert_eq!(map.len(), 2, "only the relative path goes: {:?}", map.iter().map(|(k, _)| k).collect::<Vec<_>>());
        assert!(map.iter().any(|(k, _)| k == "/home/gm/Work"));
        assert!(map.iter().any(|(k, _)| k == "/home/gm/Photos"));
        let current = from_file("{}");
        let bad = jsondoc::parse(r#"{"folderSorts":{"/a":{"key":"mode","reverse":false}}}"#).expect("patch parses");
        let message = patched(&current, &bad).expect_err("a patch from Flea must be whole");
        assert!(message.contains("folderSorts"), "got {}", message);
    }

    #[test]
    fn folder_view_patches_merge_per_folder_and_can_forget_one() {
        let current = from_file("{}");
        let a = jsondoc::parse(r#"{"folderViews":{"/a":"grid"}}"#).expect("patch parses");
        let b = jsondoc::parse(r#"{"folderViews":{"/b":"columns"}}"#).expect("patch parses");
        let both = patched(&patched(&current, &a).expect("a lands"), &b).expect("b lands");
        let map = both.get("folderViews").and_then(Json::as_object).expect("folderViews");
        assert_eq!(map.len(), 2);
        let forget = jsondoc::parse(r#"{"folderViews":{"/a":null}}"#).expect("patch parses");
        let kept = patched(&both, &forget).expect("forget lands");
        let map = kept.get("folderViews").and_then(Json::as_object).expect("folderViews");
        assert_eq!(map.len(), 1);
        assert_eq!(map.iter().next().map(|(key, value)| (key.as_str(), value.as_str())), Some(("/b", Some("columns"))));
    }

    // A relaunch reads what the last write rendered: the switch and each folder's view come back in order, and a re-view is newest.
    #[test]
    fn folder_views_survive_a_write_and_a_read_and_a_bad_entry_costs_only_itself() {
        let current = from_file("{}");
        assert_eq!(current.get("rememberViewPerFolder").and_then(Json::as_bool), Some(false), "off until the operator opts in");
        let first = jsondoc::parse(r#"{"rememberViewPerFolder":true,"folderViews":{"/a":"grid","/b":"list"}}"#).expect("patch parses");
        let again = jsondoc::parse(r#"{"folderViews":{"/a":"columns"}}"#).expect("patch parses");
        let written = patched(&patched(&current, &first).expect("first lands"), &again).expect("again lands");
        let relaunched = from_file(&jsondoc::render(&written));
        assert_eq!(relaunched.get("rememberViewPerFolder").and_then(Json::as_bool), Some(true));
        let views = |doc: &Json| doc.get("folderViews").and_then(Json::as_object).expect("folderViews").iter().map(|(path, view)| format!("{}={}", path, view.as_str().unwrap_or(""))).collect::<Vec<_>>();
        assert_eq!(views(&relaunched), ["/b=list", "/a=columns"], "both survive, the re-view newest");
        let edited = from_file(r#"{"rememberViewPerFolder":"yes","folderViews":{"/a":"grid","/b":"dual","c":"list","/d":7,"/e":"columns"}}"#);
        assert_eq!(views(&edited), ["/a=grid", "/e=columns"], "the dual layout, a relative path and a non-string go and the rest stay");
        assert_eq!(edited.get("rememberViewPerFolder").and_then(Json::as_bool), Some(false), "a switch that is no boolean reads as off");
    }

    // G1: two remembered folders both survive, a null forgets, a re-sort is newest; widths per key.
    #[test]
    fn map_patches_merge_per_entry_and_a_null_forgets() {
        let current = from_file("{}");
        let a = jsondoc::parse(r#"{"folderSorts":{"/a":{"key":"size","reverse":true}}}"#).expect("patch parses");
        let after_a = patched(&current, &a).expect("the first folder lands");
        let b = jsondoc::parse(r#"{"folderSorts":{"/b":{"key":"name","reverse":false}}}"#).expect("patch parses");
        let both = patched(&after_a, &b).expect("the second folder lands");
        let map = both.get("folderSorts").and_then(Json::as_object).expect("folderSorts");
        assert!(map.iter().any(|(k, _)| k == "/a"), "/a survives /b landing");
        assert!(map.iter().any(|(k, _)| k == "/b"), "/b landed");
        let gone = jsondoc::parse(r#"{"folderSorts":{"/a":null}}"#).expect("patch parses");
        let kept = patched(&both, &gone).expect("a null forget lands");
        let map = kept.get("folderSorts").and_then(Json::as_object).expect("folderSorts");
        assert!(!map.iter().any(|(k, _)| k == "/a"), "/a is forgotten");
        assert!(map.iter().any(|(k, _)| k == "/b"), "/b survives the forget");
        let again = jsondoc::parse(r#"{"folderSorts":{"/a":{"key":"date","reverse":false}}}"#).expect("patch parses");
        let two = patched(&kept, &again).expect("/a returns");
        let resort = jsondoc::parse(r#"{"folderSorts":{"/b":{"key":"name","reverse":false}}}"#).expect("patch parses");
        let moved = patched(&two, &resort).expect("re-sort lands");
        let keys: Vec<&str> = moved.get("folderSorts").and_then(Json::as_object).expect("folderSorts").iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["/a", "/b"], "a re-sort is the newest");
        let w = jsondoc::parse(r#"{"columnWidths":{"size":120}}"#).expect("patch parses");
        let after_w = patched(&current, &w).expect("the first width lands");
        let v = jsondoc::parse(r#"{"columnWidths":{"date":140}}"#).expect("patch parses");
        let widths = patched(&after_w, &v).expect("the second width lands");
        let map = widths.get("columnWidths").and_then(Json::as_object).expect("columnWidths");
        assert_eq!(map.len(), 2, "both widths survive");
        assert!(map.iter().any(|(k, _)| k == "size"), "size survives date landing");
    }

    fn volume_of(doc: &Json) -> Option<f64> {
        doc.get("preview").and_then(|p| p.get("volume")).and_then(Json::as_f64)
    }

    // preview.volume is the media level, 0 to 100, and 100 is the full-scale output a file without the key plays at.
    #[test]
    fn a_preview_volume_survives_a_write_and_a_file_without_one_reads_full_scale() {
        assert_eq!(volume_of(&from_file("{}")), Some(100.0));
        for level in ["0", "37", "100"] {
            let patch = jsondoc::parse(&format!(r#"{{"preview":{{"volume":{}}}}}"#, level)).expect("patch parses");
            let written = patched(&from_file("{}"), &patch).expect("a whole level lands");
            let expected = level.parse::<f64>().ok();
            assert_eq!(volume_of(&written), expected, "the patched document holds {}", level);
            assert_eq!(volume_of(&from_file(&text(&written))), expected, "{} reads back after the file is written", level);
        }
    }

    #[test]
    fn a_preview_volume_that_is_not_a_whole_level_reads_full_scale_and_a_patch_for_it_is_refused() {
        for bad in ["101", "-1", "37.5", r#""loud""#] {
            let stored = format!(r#"{{"preview":{{"volume":{},"thumbSize":"large"}}}}"#, bad);
            let read = from_file(&stored);
            assert_eq!(volume_of(&read), Some(100.0), "{} falls back to full scale", bad);
            assert_eq!(read.get("preview").and_then(|p| p.get("thumbSize")).and_then(Json::as_str), Some("large"));
            let patch = jsondoc::parse(&format!(r#"{{"preview":{{"volume":{}}}}}"#, bad)).expect("patch parses");
            let message = patched(&from_file("{}"), &patch).expect_err("a patch from Flea must be whole");
            assert!(message.contains("volume"), "got {}", message);
        }
    }

    fn width_of(doc: &Json) -> Option<f64> {
        doc.get("preview").and_then(|p| p.get("width")).and_then(Json::as_f64)
    }

    // preview.width is the last Columns column's whole percent of the area, and 0 is the default third a file without the key draws.
    #[test]
    fn a_preview_width_survives_a_write_and_a_file_without_one_reads_the_default_third() {
        assert_eq!(width_of(&from_file("{}")), Some(0.0));
        for percent in ["0", "37", "100"] {
            let patch = jsondoc::parse(&format!(r#"{{"preview":{{"width":{}}}}}"#, percent)).expect("patch parses");
            let written = patched(&from_file("{}"), &patch).expect("a whole percent lands");
            let expected = percent.parse::<f64>().ok();
            assert_eq!(width_of(&written), expected, "the patched document holds {}", percent);
            assert_eq!(width_of(&from_file(&text(&written))), expected, "{} reads back after the file is written", percent);
        }
    }

    #[test]
    fn a_preview_width_that_is_not_a_whole_percent_reads_the_default_third_and_a_patch_for_it_is_refused() {
        for bad in ["101", "-1", "37.5", r#""wide""#] {
            let stored = format!(r#"{{"preview":{{"width":{},"thumbSize":"large"}}}}"#, bad);
            let read = from_file(&stored);
            assert_eq!(width_of(&read), Some(0.0), "{} falls back to the default third", bad);
            assert_eq!(read.get("preview").and_then(|p| p.get("thumbSize")).and_then(Json::as_str), Some("large"));
            let patch = jsondoc::parse(&format!(r#"{{"preview":{{"width":{}}}}}"#, bad)).expect("patch parses");
            let message = patched(&from_file("{}"), &patch).expect_err("a patch from Flea must be whole");
            assert!(message.contains("width"), "got {}", message);
        }
    }
}
