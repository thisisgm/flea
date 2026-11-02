//! Protocol 1 is also published as data for the plugin guide's executable checks.
use crate::jsondoc::{self, Json};
use crate::ui_logic_st2::manifest::Manifest;

pub const VERSION: u64 = 1;
pub const MAX_MESSAGE: usize = 64 * 1024;
pub const MAX_ROWS: usize = 256;
pub const MAX_PROGRESS: usize = 128;
pub const MAX_PROGRESS_BYTES: usize = 1024 * 1024;
pub const MAX_SELECTION: usize = 512;
// The one start failure plugins.invalid.start's sentence describes, so the owner names the plugin for it.
pub const HELLO_TIMEOUT: &str = "plugin did not answer hello in time";
pub const CONTRACT: &str = include_str!("../../docs/plugin-protocol.json");
pub const MAX_PLACE_PATH_BYTES: usize = 4096;

/// A plugin-owned location and its absolute path within that place.
// Sample input: plugin://example.virtual-place/Photos yields ("example.virtual-place", "/Photos").
pub fn place_path(uri: &str) -> Option<(&str, &str)> {
    let (id, tail) = uri.strip_prefix("plugin://")?.split_once('/')?;
    if !crate::ui_logic_st2::manifest::is_id(id)
        || tail.len() > MAX_PLACE_PATH_BYTES
        || tail.chars().any(char::is_control)
        || tail.split('/').any(|p| p == "." || p == "..")
    {
        return None;
    }
    Some((id, &uri["plugin://".len() + id.len()..]))
}

pub const OPERATIONS: [(&str, &str, &str); 12] = [
    ("targets", "send-target", "targets"),
    ("send", "send-target", "done"),
    ("run", "menu-action", "done"),
    ("section", "rail-section", "section"),
    ("list", "place", "list"),
    ("copy", "place", "done"),
    ("move", "place", "done"),
    ("rename", "place", "done"),
    ("delete", "place", "done"),
    ("preview", "previewer", "preview"),
    ("shelf", "shelf", "done"),
    ("settings", "", "done"),
];

// Sample input: br#"{"op":"ready"}"#.
pub fn parse(line: &[u8]) -> Result<Json, String> {
    if line.len() > MAX_MESSAGE {
        return Err("plugin message exceeds 64 KiB".into());
    }
    let text = std::str::from_utf8(line).map_err(|_| "plugin message is not UTF-8")?;
    let value = jsondoc::parse(text)?;
    value
        .as_object()
        .ok_or("plugin message must be an object")?;
    Ok(value)
}

pub fn text<'a>(value: &'a Json, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Json::as_str)
        .filter(|s| s.len() <= 4096 && !s.contains('\0'))
        .ok_or_else(|| format!("invalid plugin {key}"))
}

pub fn operation(manifest: &Manifest, request: &Json) -> Result<&'static str, String> {
    let op = text(request, "op")?;
    let (_, kind, response) = OPERATIONS
        .iter()
        .find(|(name, _, _)| *name == op)
        .ok_or("unknown plugin operation")?;
    if !kind.is_empty() && manifest.block(kind).is_none() {
        return Err("plugin did not declare this kind".into());
    }
    if let Some(folder) = request.get("folder") {
        if !manifest.access.iter().any(|a| a == "folder")
            || folder
                .as_str()
                .map_or(true, |p| !p.starts_with('/') || p.contains('\0'))
        {
            return Err("plugin did not declare folder access".into());
        }
    }
    if matches!(op, "targets" | "section") && request.get("paths").is_some() {
        return Err("plugin discovery receives no selection paths".into());
    }
    if op == "run" {
        let action = text(request, "action")?;
        let actions = manifest
            .block("menu-action")
            .and_then(Json::as_array)
            .unwrap_or(&[]);
        if !actions
            .iter()
            .any(|row| row.get("id").and_then(Json::as_str) == Some(action))
        {
            return Err("plugin did not declare this action".into());
        }
        let descriptor = crate::ui_logic_st2::contributions::action_input(manifest, action)
            .map_err(|e| e.detail)?;
        match (descriptor, request.get("input")) {
            (Some(input), Some(Json::Obj(values)))
                if values.len() == 1 && values[0].0 == input.key =>
            {
                let value = values[0].1.as_str().ok_or("invalid plugin action input")?;
                if value.trim().is_empty()
                    || value.chars().count() > input.max_chars
                    || value.chars().any(char::is_control)
                {
                    return Err("invalid plugin action input".into());
                }
            }
            (None, None) => {}
            _ => return Err("plugin action input does not match its declaration".into()),
        }
    }
    if let Some(paths) = request.get("paths") {
        let paths = paths
            .as_array()
            .filter(|p| p.len() <= MAX_SELECTION)
            .ok_or("invalid plugin selection")?;
        if paths.iter().any(|p| {
            p.as_str()
                .map_or(true, |p| !p.starts_with('/') || p.contains('\0'))
        }) {
            return Err("invalid plugin selection path".into());
        }
    }
    Ok(response)
}

pub fn ready(value: &Json) -> Result<(), String> {
    if text(value, "op")? != "ready" {
        return Err("plugin did not answer ready".into());
    }
    if value
        .get("protocolVersion")
        .is_some_and(|v| v.as_f64() != Some(VERSION as f64))
    {
        return Err("unsupported plugin protocol version".into());
    }
    Ok(())
}

pub fn response(value: &Json, expected: &str) -> Result<bool, String> {
    match text(value, "op")? {
        "error" => {
            text(value, "message")?;
            Ok(true)
        }
        "progress" if expected == "done" => {
            for key in ["done", "total"] {
                let n = value
                    .get(key)
                    .and_then(Json::as_f64)
                    .ok_or("invalid plugin progress")?;
                if !(0.0..=9e15).contains(&n) || n.fract() != 0.0 {
                    return Err("invalid plugin progress".into());
                }
            }
            if value.get("done").and_then(Json::as_f64) > value.get("total").and_then(Json::as_f64)
            {
                return Err("invalid plugin progress".into());
            }
            if value.get("message").is_some() {
                text(value, "message")?;
            }
            Ok(false)
        }
        op if op == expected => {
            match op {
                "targets" => rows(value, "targets", &["id", "label", "state"]),
                "section" => rows(value, "rows", &["id", "label", "opens"]),
                "list" => rows(value, "entries", &["name", "kind"]),
                "preview" => {
                    if value.get("text").is_some() {
                        value
                            .get("text")
                            .and_then(Json::as_str)
                            .map(|_| ())
                            .ok_or_else(|| "invalid plugin preview text".into())
                    } else {
                        text(value, "png").and_then(|p| {
                            if p == "/out/preview.png" {
                                Ok(())
                            } else {
                                Err("invalid plugin preview output".into())
                            }
                        })
                    }
                }
                _ => Ok(()),
            }?;
            Ok(true)
        }
        _ => Err("unexpected plugin response".into()),
    }
}

fn rows(value: &Json, key: &str, fields: &[&str]) -> Result<(), String> {
    let rows = value
        .get(key)
        .and_then(Json::as_array)
        .filter(|r| r.len() <= MAX_ROWS)
        .ok_or_else(|| format!("invalid plugin {key}"))?;
    let mut identities = std::collections::HashSet::new();
    for row in rows {
        row.as_object().ok_or("invalid plugin row")?;
        for field in fields {
            text(row, field)?;
        }
        let identity = text(row, fields[0])?;
        if identity.is_empty() || !identities.insert(identity) {
            return Err("duplicate or empty plugin row identity".into());
        }
        if key == "entries" && (identity == "." || identity == ".." || identity.contains('/')) {
            return Err("invalid plugin entry name".into());
        }
    }
    Ok(())
}
