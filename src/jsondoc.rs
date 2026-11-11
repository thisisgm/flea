// One whole JSON document in and out, which src/json.rs deliberately is not: it scans one wire line.
use crate::json::escape;
use crate::jsonstring::parse_string;
use std::collections::HashMap;

#[cfg(test)]
#[path = "jsondoc_cost.rs"]
pub(crate) mod cost;
#[cfg(test)]
type ObjectName = cost::Name;
#[cfg(not(test))]
type ObjectName = String;

// A hand-edited state file is an input, so nesting is bounded rather than recursed until the stack ends.
pub const MAX_DEPTH: usize = 32;

pub const INDENT: &str = "  ";

// Num keeps the literal it was written with, so 192 stays 192 and 1.0 stays 1.0 across a rewrite.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs
                .iter()
                .find(|(k, _)| {
                    #[cfg(test)]
                    cost::compare(k, key);
                    k == key
                })
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(n) => n.parse::<f64>().ok(),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Json::Obj(pairs) => Some(pairs.as_slice()),
            _ => None,
        }
    }
}

// Sample input: {"view":"list","columns":["name","size"],"places":{"sidebarWidth":192}}
pub fn parse(text: &str) -> Result<Json, String> {
    let bytes = text.as_bytes();
    let mut at = skip_space(bytes, 0);
    let value = parse_value(bytes, &mut at, 0)?;
    at = skip_space(bytes, at);
    if at != bytes.len() {
        return Err(format!("trailing text at byte {}", at));
    }
    Ok(value)
}

pub fn render(value: &Json) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0);
    out.push('\n');
    out
}

// One value as it renders at `depth` inside a larger document, with no trailing newline.
pub fn render_at(value: &Json, depth: usize) -> String {
    let mut out = String::new();
    write_value(&mut out, value, depth);
    out
}

fn write_value(out: &mut String, value: &Json, depth: usize) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Num(n) => out.push_str(n),
        Json::Str(s) => {
            out.push('"');
            out.push_str(&escape(s));
            out.push('"');
        }
        Json::Arr(items) => write_items(
            out,
            items.iter().map(|v| (None, v)).collect(),
            depth,
            '[',
            ']',
        ),
        Json::Obj(pairs) => write_items(
            out,
            pairs.iter().map(|(k, v)| (Some(k.as_str()), v)).collect(),
            depth,
            '{',
            '}',
        ),
    }
}

// An empty container stays on one line, because {} reads better than a brace with nothing between.
fn write_items(
    out: &mut String,
    items: Vec<(Option<&str>, &Json)>,
    depth: usize,
    open: char,
    close: char,
) {
    out.push(open);
    if items.is_empty() {
        out.push(close);
        return;
    }
    for (i, (key, value)) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('\n');
        out.push_str(&INDENT.repeat(depth + 1));
        if let Some(k) = key {
            out.push('"');
            out.push_str(&escape(k));
            out.push_str("\": ");
        }
        write_value(out, value, depth + 1);
    }
    out.push('\n');
    out.push_str(&INDENT.repeat(depth));
    out.push(close);
}

fn skip_space(bytes: &[u8], mut at: usize) -> usize {
    while at < bytes.len() && matches!(bytes[at], b' ' | b'\t' | b'\n' | b'\r') {
        at += 1;
    }
    at
}

fn parse_value(bytes: &[u8], at: &mut usize, depth: usize) -> Result<Json, String> {
    if depth >= MAX_DEPTH {
        return Err(format!("nested past {} levels at byte {}", MAX_DEPTH, at));
    }
    match bytes.get(*at) {
        None => Err(format!("the document ended before a value at byte {}", at)),
        Some(b'{') => parse_object(bytes, at, depth),
        Some(b'[') => parse_array(bytes, at, depth),
        Some(b'"') => parse_string(bytes, at).map(Json::Str),
        Some(b't') => literal(bytes, at, "true", Json::Bool(true)),
        Some(b'f') => literal(bytes, at, "false", Json::Bool(false)),
        Some(b'n') => literal(bytes, at, "null", Json::Null),
        Some(_) => parse_number(bytes, at),
    }
}

fn literal(bytes: &[u8], at: &mut usize, word: &str, value: Json) -> Result<Json, String> {
    if bytes[*at..].starts_with(word.as_bytes()) {
        *at += word.len();
        return Ok(value);
    }
    Err(format!("{} expected at byte {}", word, at))
}

fn parse_object(bytes: &[u8], at: &mut usize, depth: usize) -> Result<Json, String> {
    *at += 1;
    // A hashed name-to-slot index keeps a wide or duplicate-heavy object linear; an emptied slot is a replaced name.
    let mut slots: Vec<Option<(ObjectName, Json)>> = Vec::new();
    let mut named: HashMap<ObjectName, usize> = HashMap::new();
    *at = skip_space(bytes, *at);
    if bytes.get(*at) == Some(&b'}') {
        *at += 1;
        return Ok(Json::Obj(Vec::new()));
    }
    loop {
        *at = skip_space(bytes, *at);
        let key = parse_string(bytes, at)?;
        #[cfg(test)]
        let key = cost::Name::from(key);
        *at = skip_space(bytes, *at);
        if bytes.get(*at) != Some(&b':') {
            return Err(format!("a colon was expected at byte {}", at));
        }
        *at += 1;
        *at = skip_space(bytes, *at);
        let value = parse_value(bytes, at, depth + 1)?;
        // A repeated name is the later one's, the same reading every JSON parser takes.
        if let Some(earlier) = named.insert(key.clone(), slots.len()) {
            slots[earlier] = None;
        }
        slots.push(Some((key, value)));
        *at = skip_space(bytes, *at);
        match bytes.get(*at) {
            Some(b',') => *at += 1,
            Some(b'}') => {
                *at += 1;
                let pairs = slots.into_iter().flatten();
                #[cfg(test)]
                let pairs = pairs.map(|(key, value)| (String::from(key), value));
                return Ok(Json::Obj(pairs.collect()));
            }
            _ => {
                return Err(format!(
                    "a comma or a closing brace was expected at byte {}",
                    at
                ))
            }
        }
    }
}

fn parse_array(bytes: &[u8], at: &mut usize, depth: usize) -> Result<Json, String> {
    *at += 1;
    let mut items: Vec<Json> = Vec::new();
    *at = skip_space(bytes, *at);
    if bytes.get(*at) == Some(&b']') {
        *at += 1;
        return Ok(Json::Arr(items));
    }
    loop {
        *at = skip_space(bytes, *at);
        items.push(parse_value(bytes, at, depth + 1)?);
        *at = skip_space(bytes, *at);
        match bytes.get(*at) {
            Some(b',') => *at += 1,
            Some(b']') => {
                *at += 1;
                return Ok(Json::Arr(items));
            }
            _ => {
                return Err(format!(
                    "a comma or a closing bracket was expected at byte {}",
                    at
                ))
            }
        }
    }
}

// Sample input: -12, 0, 192, 1.0, 1.5e-3; the literal is kept as written, and checked for JSON's
// own shape and a finite value rather than rewritten into a canonical one.
fn parse_number(bytes: &[u8], at: &mut usize) -> Result<Json, String> {
    let start = *at;
    while let Some(&b) = bytes.get(*at) {
        if b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E') {
            *at += 1;
            continue;
        }
        break;
    }
    let text = std::str::from_utf8(&bytes[start..*at])
        .map_err(|_| format!("a number that is not UTF-8 at byte {}", start))?;
    // Rust's own f64 parse takes a leading plus, a leading zero, a bare fraction and a trailing dot
    // where JSON takes none of them, and the literal is written back verbatim, so one accepted here
    // would put a value in the file that this cannot read again.
    if !is_json_number(text) {
        return Err(format!("a value that is not JSON at byte {}", start));
    }
    // The shape is JSON's and the parse is the magnitude: 1e400 is a JSON number and no finite f64.
    match text.parse::<f64>() {
        Ok(n) if n.is_finite() => Ok(Json::Num(text.to_string())),
        _ => Err(format!("a value that is not JSON at byte {}", start)),
    }
}

// JSON's whole number grammar: -? (0 | [1-9][0-9]*) (. [0-9]+)? ([eE] [+-]? [0-9]+)? and nothing else.
fn is_json_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = if bytes.first() == Some(&b'-') { 1 } else { 0 };
    let int_end = digit_run(bytes, at);
    // A leading zero is a whole integer part on its own, so 0192 is Rust's 192 and is not JSON at all.
    if int_end == at || (int_end > at + 1 && bytes[at] == b'0') {
        return false;
    }
    at = int_end;
    if bytes.get(at) == Some(&b'.') {
        let frac_end = digit_run(bytes, at + 1);
        if frac_end == at + 1 {
            return false;
        }
        at = frac_end;
    }
    if !matches!(bytes.get(at), Some(b'e' | b'E')) {
        return at == bytes.len();
    }
    at += 1;
    if matches!(bytes.get(at), Some(b'+' | b'-')) {
        at += 1;
    }
    let exp_end = digit_run(bytes, at);
    exp_end > at && exp_end == bytes.len()
}

// The end of the run of ASCII digits at `at`, which is `at` itself when there is no digit there.
fn digit_run(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    at
}

#[cfg(test)]
#[path = "jsondoc_tests.rs"]
mod tests;
