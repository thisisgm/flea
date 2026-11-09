//! Kind blocks retained by the manifest and mapped to existing surface models.
use super::manifest::{Invalid, Manifest, KINDS};
use crate::jsondoc::Json;
use crate::ui_logic_custom_actions::plugins::{Contribution, Menu, When};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionInput {
    pub key: String,
    pub title: String,
    pub label: String,
    pub placeholder: String,
    pub submit: String,
    pub max_chars: usize,
}

pub fn action_input(manifest: &Manifest, action: &str) -> Result<Option<ActionInput>, Invalid> {
    let Some(actions) = manifest.block("menu-action").and_then(Json::as_array) else {
        return Ok(None);
    };
    let Some(action) = actions
        .iter()
        .find(|row| row.get("id").and_then(Json::as_str) == Some(action))
    else {
        return Ok(None);
    };
    input(action)
}

fn input(action: &Json) -> Result<Option<ActionInput>, Invalid> {
    let Some(value) = action.get("input") else {
        return Ok(None);
    };
    value
        .as_object()
        .ok_or_else(|| Invalid::new("block", "input"))?;
    let key = text(value, "key")?;
    if key.len() > 64
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err(Invalid::new("block", "input.key"));
    }
    let title = text(value, "title")?;
    let label = text(value, "label")?;
    let submit = text(value, "submit")?;
    if title.chars().count() > 80 || label.chars().count() > 80 || submit.chars().count() > 24 {
        return Err(Invalid::new("block", "input.label"));
    }
    let max_chars = match value.get("maxChars") {
        None => 4096,
        Some(n) => n
            .as_f64()
            .filter(|n| (1.0..=4096.0).contains(n) && n.fract() == 0.0)
            .map(|n| n as usize)
            .ok_or_else(|| Invalid::new("block", "input.maxChars"))?,
    };
    let placeholder = value
        .get("placeholder")
        .and_then(Json::as_str)
        .unwrap_or("");
    if placeholder.chars().count() > 256 || placeholder.chars().any(char::is_control) {
        return Err(Invalid::new("block", "input.placeholder"));
    }
    Ok(Some(ActionInput {
        key: key.into(),
        title: title.into(),
        label: label.into(),
        placeholder: placeholder.into(),
        submit: submit.into(),
        max_chars,
    }))
}

fn text<'a>(value: &'a Json, key: &str) -> Result<&'a str, Invalid> {
    value
        .get(key)
        .and_then(Json::as_str)
        .filter(|s| {
            !s.trim().is_empty() && s.chars().count() <= 512 && !s.chars().any(char::is_control)
        })
        .ok_or_else(|| Invalid::new("block", key))
}

fn string_list(value: &Json, key: &str) -> Result<Vec<String>, Invalid> {
    let items = value
        .get(key)
        .and_then(Json::as_array)
        .filter(|a| a.len() <= 64)
        .ok_or_else(|| Invalid::new("block", key))?;
    items
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
                .map(str::to_owned)
                .ok_or_else(|| Invalid::new("block", key))
        })
        .collect()
}

fn count(value: &Json, key: &str, default: usize) -> Result<usize, Invalid> {
    match value.get(key) {
        None => Ok(default),
        Some(n) => n
            .as_f64()
            .filter(|n| (0.0..=512.0).contains(n) && n.fract() == 0.0)
            .map(|n| n as usize)
            .ok_or_else(|| Invalid::new("block", key)),
    }
}

pub(super) fn validate(doc: &Json, kinds: &[String]) -> Result<Vec<(String, Json)>, Invalid> {
    let mut blocks = Vec::new();
    for (kind, key) in KINDS {
        let Some(value) = doc.get(key) else {
            continue;
        };
        if !kinds.iter().any(|k| k == kind) {
            return Err(Invalid::new("block", key));
        }
        match kind {
            "menu-action" => {
                actions("", value)?;
            }
            _ => {
                value
                    .as_object()
                    .ok_or_else(|| Invalid::new("block", key))?;
                match kind {
                    "send-target" | "place" => {
                        text(value, "label")?;
                    }
                    "rail-section" => {
                        text(value, "title")?;
                    }
                    "previewer" => {
                        if string_list(value, "mime")?.is_empty() {
                            return Err(Invalid::new("block", "mime"));
                        }
                        if !matches!(text(value, "output")?, "png" | "text") {
                            return Err(Invalid::new("block", "output"));
                        }
                    }
                    "shelf" => {
                        let name = text(value, "omarchyPlugin")?;
                        if name.starts_with('-')
                            || !name
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                        {
                            return Err(Invalid::new("block", "omarchyPlugin"));
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
        blocks.push((key.to_owned(), value.clone()));
    }
    Ok(blocks)
}

pub fn menu_actions(manifest: &Manifest) -> Result<Vec<Contribution>, Invalid> {
    manifest
        .block("menu-action")
        .map_or_else(|| Ok(Vec::new()), |block| actions(&manifest.id, block))
}

fn actions(plugin: &str, block: &Json) -> Result<Vec<Contribution>, Invalid> {
    let actions = block
        .as_array()
        .filter(|a| a.len() <= 64)
        .ok_or_else(|| Invalid::new("block", "menuActions"))?;
    let mut found: Vec<Contribution> = Vec::new();
    for action in actions {
        input(action)?;
        let id = text(action, "id")?;
        if id.contains(':') || found.iter().any(|c| c.action == id) {
            return Err(Invalid::new("block", "id"));
        }
        let when = action.get("when").cloned().unwrap_or(Json::Obj(Vec::new()));
        when.as_object()
            .ok_or_else(|| Invalid::new("block", "when"))?;
        let min_items = count(&when, "minItems", 1)?;
        let max_items = count(&when, "maxItems", 512)?;
        if max_items < min_items {
            return Err(Invalid::new("block", "maxItems"));
        }
        let folders = match when.get("folders") {
            None => false,
            Some(v) => v
                .as_bool()
                .ok_or_else(|| Invalid::new("block", "folders"))?,
        };
        let menus = match action.get("menus") {
            None => vec![Menu::File],
            Some(_) => string_list(action, "menus")?
                .iter()
                .map(|m| match m.as_str() {
                    "file" => Ok(Menu::File),
                    "background" => Ok(Menu::Background),
                    _ => Err(Invalid::new("block", "menus")),
                })
                .collect::<Result<Vec<_>, _>>()?,
        };
        found.push(Contribution {
            plugin: plugin.into(),
            action: id.into(),
            label: text(action, "label")?.into(),
            glyph: action
                .get("glyph")
                .and_then(Json::as_str)
                .unwrap_or("code")
                .to_owned(),
            menus,
            when: When {
                min_items,
                max_items: Some(max_items),
                folders,
                mime: if when.get("mime").is_some() {
                    string_list(&when, "mime")?
                } else {
                    Vec::new()
                },
            },
        });
    }
    Ok(found)
}
