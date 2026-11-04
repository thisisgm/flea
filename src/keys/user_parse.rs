//! Parses the user binding file in the same table shape as keys.toml.
use super::{is_plugin_id, Problem, UserBinding, UserKeys, MAX_BINDINGS, MAX_BYTES, MODS, SHARED};
use crate::keys::{PRESETS, PRESET_KEYS, SHARED_KEYS};

fn known_key(key: &str) -> bool {
    let mut chars = key.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => c.is_ascii_alphanumeric(),
        (Some(_), Some(_)) => {
            key.strip_prefix('F')
                .and_then(|n| n.parse::<u8>().ok())
                .is_some_and(|n| (1..=24).contains(&n) && key == format!("F{n}"))
                || PRESET_KEYS
                    .iter()
                    .chain(SHARED_KEYS)
                    .any(|row| row.mods != "text" && row.key.len() > 1 && row.key == key)
        }
        _ => false,
    }
}

fn text_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_control() && !c.is_whitespace())
}

// Sample input: "K" # optional comment; double quotes, with the \" \\ \t \n escapes, and nothing but a comment after it.
fn string(value: &str) -> Option<String> {
    let rest = value.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let tail = chars.as_str().trim_start();
                return (tail.is_empty() || tail.starts_with('#')).then_some(out);
            }
            '\\' => out.push(match chars.next()? {
                '"' => '"',
                '\\' => '\\',
                't' => '\t',
                'n' => '\n',
                _ => return None,
            }),
            c if c.is_control() => return None,
            c => out.push(c),
        }
    }
    None
}

struct Block {
    table: String,
    line: usize,
    fields: Vec<(String, String)>,
    invalid: bool,
}

// Sample input: [[ctrlshift]] with key = "K" and action = "example.checksums:sha256".
fn binding(block: &Block) -> Result<UserBinding, String> {
    let field = |name: &str| {
        block
            .fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    let allowed: &[&str] = if block.table == "preset" {
        &["name", "mods", "key", "action"]
    } else {
        &["key", "action"]
    };
    if let Some((name, _)) = block
        .fields
        .iter()
        .find(|(k, _)| !allowed.contains(&k.as_str()))
    {
        return Err(format!("unknown field {name}"));
    }
    let (preset, mods) = if block.table == "preset" {
        let name = field("name").unwrap_or("all");
        if name != "all" && !PRESETS.contains(&name) {
            return Err(format!("unknown preset {name}"));
        }
        let mods = field("mods").ok_or("missing mods")?;
        (name.to_owned(), mods)
    } else {
        let mods = SHARED
            .iter()
            .find(|(table, _)| *table == block.table)
            .map(|(_, mods)| *mods)
            .ok_or_else(|| format!("unknown table {}", block.table))?;
        ("all".to_owned(), mods)
    };
    let (mods, mask) = MODS
        .iter()
        .copied()
        .find(|(name, _)| *name == mods)
        .ok_or_else(|| format!("unknown mods {mods}"))?;
    let key = field("key").ok_or("missing key")?;
    let action = field("action").ok_or("missing action")?;
    if mods == "text" && !text_key(key) || mods != "text" && !known_key(key) {
        return Err(format!("unknown key {key}"));
    }
    if action.is_empty()
        || action.chars().any(|c| c.is_control() || c.is_whitespace())
        || action.contains(':') && !is_plugin_id(action)
    {
        return Err(format!("unreadable action {action}"));
    }
    Ok(UserBinding {
        preset,
        mods,
        mask,
        key: key.to_owned(),
        action: action.to_owned(),
        line: block.line,
    })
}

// Sample input: [[ctrlshift]]\nkey = "K"\naction = "example.checksums:sha256"
pub fn parse(text: &str) -> UserKeys {
    let mut out = UserKeys::default();
    if text.len() > MAX_BYTES {
        out.problems.push(Problem {
            line: 0,
            detail: format!("larger than {MAX_BYTES} bytes"),
        });
        return out;
    }
    let mut blocks: Vec<Block> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some(name) = trimmed
            .strip_prefix("[[")
            .and_then(|rest| rest.split_once("]]"))
            .filter(|(_, tail)| tail.trim().is_empty() || tail.trim().starts_with('#'))
            .map(|(name, _)| name.trim())
        {
            blocks.push(Block {
                table: name.to_owned(),
                line,
                fields: Vec::new(),
                invalid: false,
            });
            continue;
        }
        let parsed = trimmed
            .split_once('=')
            .and_then(|(name, value)| Some((name.trim(), string(value.trim())?)));
        match (parsed, blocks.last_mut()) {
            (Some((name, value)), Some(block))
                if !name.is_empty() && !block.fields.iter().any(|(k, _)| k == name) =>
            {
                block.fields.push((name.to_owned(), value))
            }
            (_, block) => {
                if let Some(block) = block {
                    block.invalid = true;
                }
                out.problems.push(Problem {
                    line,
                    detail: "not a key = \"value\" line inside a [[table]] block".into(),
                });
            }
        }
    }
    for block in &blocks {
        if out.bindings.len() == MAX_BINDINGS {
            out.problems.push(Problem {
                line: block.line,
                detail: format!("more than {MAX_BINDINGS} bindings"),
            });
            break;
        }
        match binding(block) {
            Ok(binding) if !block.invalid => out.bindings.push(binding),
            Ok(_) => {}
            Err(detail) => out.problems.push(Problem {
                line: block.line,
                detail,
            }),
        }
    }
    out.problems.sort_by_key(|problem| problem.line);
    out
}
