//! A manifest's settings fields in Omarchy's types, and the clamp every stored value passes on read.
use super::manifest::Invalid;
use crate::jsondoc::Json;

const MAX_STRING: usize = 4096;
const MAX_OPTIONS: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Boolean { default: bool },
    Integer { min: i64, max: i64, step: i64, default: i64, unit: String },
    Text { default: String, placeholder: String },
    // Each option is its stored value and its label.
    Choice { options: Vec<(String, String)>, default: String },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub hint: String,
    pub kind: Kind,
}

// What a field holds after the clamp; never out of range and never another type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Text(String),
    Choice(String),
}

impl Value {
    pub fn to_json(&self) -> Json {
        match self {
            Value::Bool(b) => Json::Bool(*b),
            Value::Int(n) => Json::Num(n.to_string()),
            Value::Text(s) | Value::Choice(s) => Json::Str(s.clone()),
        }
    }
}

fn whole(value: Option<&Json>) -> Option<i64> {
    let n = value?.as_f64()?;
    (n.is_finite() && n.fract() == 0.0 && n.abs() < 9.0e15).then_some(n as i64)
}

fn label(item: &Json, key: &str) -> Result<String, Invalid> {
    match item.get("label").and_then(Json::as_str) {
        Some(label) if !label.trim().is_empty() => Ok(label.to_string()),
        _ => Err(Invalid::new("field", key)),
    }
}

// Sample input: {"key":"timeoutSec","type":"integer","label":"Timeout","min":2,"max":60,"step":1,"defaultValue":8}
pub fn field(item: &Json) -> Result<Field, Invalid> {
    let key = item.get("key").and_then(Json::as_str).unwrap_or("");
    let key_ok = !key.is_empty() && key.len() <= 64 && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !key_ok {
        return Err(Invalid::new("field", key));
    }
    let bad = || Invalid::new("field", key);
    let default = item.get("defaultValue");
    let kind = match item.get("type").and_then(Json::as_str) {
        Some("boolean") => Kind::Boolean { default: default.and_then(Json::as_bool).ok_or_else(bad)? },
        Some("integer") => {
            let min = whole(item.get("min")).ok_or_else(bad)?;
            let max = whole(item.get("max")).ok_or_else(bad)?;
            let step = whole(item.get("step")).unwrap_or(1);
            let value = whole(default).ok_or_else(bad)?;
            if min > max || step < 1 || value < min || value > max {
                return Err(bad());
            }
            let unit = item.get("unit").and_then(Json::as_str).unwrap_or("").to_string();
            Kind::Integer { min, max, step, default: value, unit }
        }
        Some("string") => Kind::Text {
            default: default.and_then(Json::as_str).unwrap_or("").chars().take(MAX_STRING).collect(),
            placeholder: item.get("placeholder").and_then(Json::as_str).unwrap_or("").to_string(),
        },
        Some("choice") => {
            let options: Vec<(String, String)> = item
                .get("options")
                .and_then(Json::as_array)
                .ok_or_else(bad)?
                .iter()
                .map(|o| match (o.get("value").and_then(Json::as_str), o.get("label").and_then(Json::as_str)) {
                    (Some(v), Some(l)) if !v.is_empty() => Ok((v.to_string(), l.to_string())),
                    _ => Err(bad()),
                })
                .collect::<Result<_, _>>()?;
            let chosen = default.and_then(Json::as_str).ok_or_else(bad)?;
            if options.is_empty() || options.len() > MAX_OPTIONS || !options.iter().any(|(v, _)| v == chosen) {
                return Err(bad());
            }
            Kind::Choice { options, default: chosen.to_string() }
        }
        _ => return Err(bad()),
    };
    Ok(Field { key: key.to_string(), label: label(item, key)?, hint: item.get("hint").and_then(Json::as_str).unwrap_or("").to_string(), kind })
}

// A hand-edited or stale value never reaches the plugin: wrong types fall back, numbers land on the nearest step inside the range.
pub fn clamp(field: &Field, stored: Option<&Json>) -> Value {
    match &field.kind {
        Kind::Boolean { default } => Value::Bool(stored.and_then(Json::as_bool).unwrap_or(*default)),
        Kind::Integer { min, max, step, default, .. } => {
            let raw = match stored.and_then(Json::as_f64) {
                Some(n) if n.is_finite() => n,
                _ => return Value::Int(*default),
            };
            // The ends are kept even off the step grid, so max stays reachable.
            if raw >= *max as f64 {
                return Value::Int(*max);
            }
            if raw <= *min as f64 {
                return Value::Int(*min);
            }
            let steps = ((raw - *min as f64) / *step as f64).round() as i64;
            Value::Int((*min + steps * *step).min(*max))
        }
        Kind::Text { default, .. } => match stored.and_then(Json::as_str) {
            Some(s) if s.chars().count() <= MAX_STRING && !s.contains('\0') => Value::Text(s.to_string()),
            _ => Value::Text(default.clone()),
        },
        Kind::Choice { options, default } => match stored.and_then(Json::as_str) {
            Some(s) if options.iter().any(|(v, _)| v == s) => Value::Choice(s.to_string()),
            _ => Value::Choice(default.clone()),
        },
    }
}

// Every field clamped, in manifest order; the object the hello and settings lines carry.
pub fn settings(fields: &[Field], stored: Option<&Json>) -> Vec<(String, Value)> {
    fields.iter().map(|f| (f.key.clone(), clamp(f, stored.and_then(|s| s.get(&f.key))))).collect()
}
