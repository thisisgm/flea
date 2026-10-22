// ui.json's plugins map: a plugin id to its own switch and raw settings, which the manifest re-clamps on every read.
use crate::jsondoc::Json;

// A manifest declares far fewer; this only bounds what a hand-edited file can make Flea carry.
const MAX_PLUGINS: usize = 256;
const MAX_SETTINGS: usize = 64;

fn scalar(value: &Json) -> bool {
    matches!(value, Json::Bool(_) | Json::Num(_) | Json::Str(_))
}

// Sample input: {"enabled":true,"settings":{"sum":"sha256"}}
fn entry_fits(value: &Json, patch: bool) -> bool {
    let Some(pairs) = value.as_object() else { return false; };
    pairs.iter().all(|(key, inner)| match key.as_str() {
        "enabled" => inner.as_bool().is_some(),
        "settings" => inner.as_object().is_some_and(|s| {
            s.len() <= MAX_SETTINGS && s.iter().all(|(_, v)| scalar(v) || (patch && matches!(v, Json::Null)))
        }),
        _ => false,
    })
}

pub fn fits(value: &Json) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.len() <= MAX_PLUGINS
            && pairs.iter().all(|(id, entry)| flea::ui_logic_st2::manifest::is_id(id) && entry_fits(entry, false))
    })
}

// A null plugin forgets it whole and a null setting forgets that key; only a patch may send one.
pub fn is_patch(value: &Json) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.iter().all(|(id, entry)| {
            flea::ui_logic_st2::manifest::is_id(id) && (matches!(entry, Json::Null) || entry_fits(entry, true))
        })
    })
}

// A hand-edited map heals one bad entry rather than losing every plugin's state.
pub fn clean(value: &Json) -> Json {
    let pairs = value.as_object().unwrap_or(&[]);
    let kept: Vec<(String, Json)> = pairs
        .iter()
        .filter(|(id, entry)| flea::ui_logic_st2::manifest::is_id(id) && entry_fits(entry, false))
        .take(MAX_PLUGINS)
        .cloned()
        .collect();
    Json::Obj(kept)
}

fn merged_entry(held: Option<&Json>, patch: &Json) -> Json {
    let mut out: Vec<(String, Json)> = held.and_then(Json::as_object).map(<[(String, Json)]>::to_vec).unwrap_or_default();
    for (key, value) in patch.as_object().unwrap_or(&[]) {
        let next = if key == "settings" {
            let mut settings: Vec<(String, Json)> =
                out.iter().find(|(k, _)| k == "settings").and_then(|(_, v)| v.as_object()).map(<[(String, Json)]>::to_vec).unwrap_or_default();
            for (name, setting) in value.as_object().unwrap_or(&[]) {
                settings.retain(|(k, _)| k != name);
                if !matches!(setting, Json::Null) {
                    settings.push((name.clone(), setting.clone()));
                }
            }
            Json::Obj(settings)
        } else {
            value.clone()
        };
        match out.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = next,
            None => out.push((key.clone(), next)),
        }
    }
    Json::Obj(out)
}

// A plugin patch names only what changed, so one window's write keeps what another wrote since its read.
pub fn merged(held: Option<Json>, patch: &Json) -> Json {
    let mut pairs: Vec<(String, Json)> = held.as_ref().and_then(Json::as_object).map(<[(String, Json)]>::to_vec).unwrap_or_default();
    for (id, entry) in patch.as_object().unwrap_or(&[]) {
        let before = pairs.iter().find(|(k, _)| k == id).map(|(_, v)| v.clone());
        pairs.retain(|(k, _)| k != id);
        if !matches!(entry, Json::Null) {
            pairs.push((id.clone(), merged_entry(before.as_ref(), entry)));
        }
    }
    clean(&Json::Obj(pairs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsondoc::parse;

    #[test]
    fn a_plugins_map_holds_valid_ids_with_a_switch_and_scalar_settings() {
        assert!(fits(&parse(r#"{"mtorvik.checksums":{"enabled":true,"settings":{"sum":"sha256","copy":true,"n":3}}}"#).unwrap()));
        assert!(fits(&parse("{}").unwrap()));
        assert!(!fits(&parse(r#"{"Checksums":{"enabled":true}}"#).unwrap()), "an id is lowercase and namespaced");
        assert!(!fits(&parse(r#"{"a.b":{"enabled":"yes"}}"#).unwrap()));
        assert!(!fits(&parse(r#"{"a.b":{"settings":{"x":[1]}}}"#).unwrap()), "a setting is a scalar");
        assert!(!fits(&parse(r#"{"a.b":{"other":1}}"#).unwrap()));
        assert!(!fits(&parse(r#"{"a.b":{"settings":{"x":null}}}"#).unwrap()), "a stored null is only a patch's");
    }

    #[test]
    fn a_patch_merges_per_plugin_and_per_setting_and_null_forgets() {
        let held = parse(r#"{"a.b":{"enabled":true,"settings":{"x":1,"y":2}},"c.d":{"enabled":false}}"#).unwrap();
        let patch = parse(r#"{"a.b":{"settings":{"x":5,"y":null}},"c.d":null,"e.f":{"enabled":true}}"#).unwrap();
        assert!(is_patch(&patch));
        let next = merged(Some(held), &patch);
        assert_eq!(crate::jsondoc::render(&next).split_whitespace().collect::<String>(),
                   r#"{"a.b":{"enabled":true,"settings":{"x":5}},"e.f":{"enabled":true}}"#);
        assert!(!is_patch(&parse(r#"{"BAD":null}"#).unwrap()));
    }

    #[test]
    fn a_hand_edited_map_keeps_its_good_entries() {
        let cleaned = clean(&parse(r#"{"a.b":{"enabled":true},"bad":{"enabled":true},"c.d":{"enabled":1}}"#).unwrap());
        assert_eq!(crate::jsondoc::render(&cleaned).split_whitespace().collect::<String>(), r#"{"a.b":{"enabled":true}}"#);
        assert_eq!(clean(&parse("[]").unwrap()), Json::Obj(Vec::new()));
    }
}
