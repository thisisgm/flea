// One whole JSON document in and out, which src/json.rs deliberately is not: it scans one wire line.
use crate::json::escape;

// A hand-edited state file is an input, so nesting is bounded rather than recursed until the stack ends.
pub const MAX_DEPTH: usize = 32;

const INDENT: &str = "  ";

// UTF-16's surrogate halves, which are not scalar values and only mean anything as a pair.
const HIGH_FIRST: u32 = 0xd800;
const HIGH_LAST: u32 = 0xdbff;
const LOW_FIRST: u32 = 0xdc00;
const LOW_LAST: u32 = 0xdfff;

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
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
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
        Json::Arr(items) => write_items(out, items.iter().map(|v| (None, v)).collect(), depth, '[', ']'),
        Json::Obj(pairs) => write_items(out, pairs.iter().map(|(k, v)| (Some(k.as_str()), v)).collect(), depth, '{', '}'),
    }
}

// An empty container stays on one line, because {} reads better than a brace with nothing between.
fn write_items(out: &mut String, items: Vec<(Option<&str>, &Json)>, depth: usize, open: char, close: char) {
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
        None => Err("the document ended before a value".to_string()),
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
    let mut pairs: Vec<(String, Json)> = Vec::new();
    *at = skip_space(bytes, *at);
    if bytes.get(*at) == Some(&b'}') {
        *at += 1;
        return Ok(Json::Obj(pairs));
    }
    loop {
        *at = skip_space(bytes, *at);
        let key = parse_string(bytes, at)?;
        *at = skip_space(bytes, *at);
        if bytes.get(*at) != Some(&b':') {
            return Err(format!("a colon was expected at byte {}", at));
        }
        *at += 1;
        *at = skip_space(bytes, *at);
        let value = parse_value(bytes, at, depth + 1)?;
        // A repeated name is the later one's, the same reading every JSON parser takes.
        pairs.retain(|(k, _)| k != &key);
        pairs.push((key, value));
        *at = skip_space(bytes, *at);
        match bytes.get(*at) {
            Some(b',') => *at += 1,
            Some(b'}') => {
                *at += 1;
                return Ok(Json::Obj(pairs));
            }
            _ => return Err(format!("a comma or a closing brace was expected at byte {}", at)),
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
            _ => return Err(format!("a comma or a closing bracket was expected at byte {}", at)),
        }
    }
}

fn parse_string(bytes: &[u8], at: &mut usize) -> Result<String, String> {
    if bytes.get(*at) != Some(&b'"') {
        return Err(format!("a string was expected at byte {}", at));
    }
    *at += 1;
    let mut out = String::new();
    while let Some(&b) = bytes.get(*at) {
        *at += 1;
        match b {
            b'"' => return Ok(out),
            b'\\' => out.push(unescape(bytes, at)?),
            _ => {
                let start = *at - 1;
                while *at < bytes.len() && bytes[*at] & 0xc0 == 0x80 {
                    *at += 1;
                }
                match std::str::from_utf8(&bytes[start..*at]) {
                    Ok(s) => out.push_str(s),
                    Err(_) => return Err(format!("a byte that is not UTF-8 at {}", start)),
                }
            }
        }
    }
    Err("a string ran to the end of the document".to_string())
}

fn unescape(bytes: &[u8], at: &mut usize) -> Result<char, String> {
    let code = *bytes.get(*at).ok_or_else(|| "an escape ran off the end".to_string())?;
    *at += 1;
    match code {
        b'"' => Ok('"'),
        b'\\' => Ok('\\'),
        b'/' => Ok('/'),
        b'b' => Ok('\u{8}'),
        b'f' => Ok('\u{c}'),
        b'n' => Ok('\n'),
        b'r' => Ok('\r'),
        b't' => Ok('\t'),
        b'u' => unescape_hex(bytes, at),
        _ => Err(format!("an unknown escape at byte {}", *at - 1)),
    }
}

// Sample input: the four hex digits after \u, as in é, or the \ud83d\udcc1 pair every ASCII-safe
// serializer writes 📁 as; a surrogate with no partner becomes the replacement character.
fn unescape_hex(bytes: &[u8], at: &mut usize) -> Result<char, String> {
    let point = hex4(bytes, at)?;
    if let Some(low) = low_half(bytes, at, point) {
        let combined = 0x10000 + ((point - HIGH_FIRST) << 10) + (low - LOW_FIRST);
        return Ok(char::from_u32(combined).unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    Ok(char::from_u32(point).unwrap_or(char::REPLACEMENT_CHARACTER))
}

fn hex4(bytes: &[u8], at: &mut usize) -> Result<u32, String> {
    let end = *at + 4;
    let digits = bytes.get(*at..end).ok_or_else(|| "a short \\u escape".to_string())?;
    let text = std::str::from_utf8(digits).map_err(|_| "a \\u escape that is not hex".to_string())?;
    let point = u32::from_str_radix(text, 16).map_err(|_| format!("a \\u escape that is not hex at byte {}", at))?;
    *at = end;
    Ok(point)
}

// The partner of a high surrogate, consumed only when the next escape really is a low one.
fn low_half(bytes: &[u8], at: &mut usize, high: u32) -> Option<u32> {
    if !(HIGH_FIRST..=HIGH_LAST).contains(&high) || bytes.get(*at) != Some(&b'\\') || bytes.get(*at + 1) != Some(&b'u') {
        return None;
    }
    let mut after = *at + 2;
    let low = hex4(bytes, &mut after).ok()?;
    if !(LOW_FIRST..=LOW_LAST).contains(&low) {
        return None;
    }
    *at = after;
    Some(low)
}

// Sample input: -12, 0, 192, 1.0, 1.5e-3; the literal is kept as written and only checked for shape.
fn parse_number(bytes: &[u8], at: &mut usize) -> Result<Json, String> {
    let start = *at;
    // Rust's own f64 parse takes a leading plus and JSON does not, and the literal is written back
    // verbatim, so accepting one here would put a value in the file that this cannot read again.
    match bytes.get(start) {
        Some(b'-') => {}
        Some(b) if b.is_ascii_digit() => {}
        _ => return Err(format!("a value that is not JSON at byte {}", start)),
    }
    while let Some(&b) = bytes.get(*at) {
        if b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e' | b'E') {
            *at += 1;
            continue;
        }
        break;
    }
    let text = std::str::from_utf8(&bytes[start..*at]).map_err(|_| format!("a number that is not UTF-8 at byte {}", start))?;
    match text.parse::<f64>() {
        Ok(n) if n.is_finite() => Ok(Json::Num(text.to_string())),
        _ => Err(format!("a value that is not JSON at byte {}", start)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_value_kind_and_keeps_object_order() {
        let v = parse(r#"{"b":true,"n":null,"i":192,"f":1.0,"s":"x","a":[1,"two",false],"o":{"k":"v"}}"#)
            .expect("parse");
        let keys: Vec<&str> = v.as_object().expect("object").iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["b", "n", "i", "f", "s", "a", "o"]);
        assert_eq!(v.get("b").and_then(Json::as_bool), Some(true));
        assert_eq!(v.get("n"), Some(&Json::Null));
        assert_eq!(v.get("i").and_then(Json::as_f64), Some(192.0));
        assert_eq!(v.get("s").and_then(Json::as_str), Some("x"));
        assert_eq!(v.get("a").and_then(Json::as_array).map(<[Json]>::len), Some(3));
        assert_eq!(v.get("o").and_then(|o| o.get("k")).and_then(Json::as_str), Some("v"));
    }

    #[test]
    fn a_number_keeps_the_literal_it_was_written_with() {
        let v = parse(r#"{"width":192,"opacity":1.0}"#).expect("parse");
        assert_eq!(render(&v), "{\n  \"width\": 192,\n  \"opacity\": 1.0\n}\n");
    }

    #[test]
    fn strings_survive_a_round_trip_with_every_escape_in_them() {
        let v = parse(r#"{"k":"a\"b\\c\nd\teAé\/f"}"#).expect("parse");
        assert_eq!(v.get("k").and_then(Json::as_str), Some("a\"b\\c\nd\te\u{41}\u{e9}/f"));
        assert_eq!(parse(&render(&v)).expect("reparse"), v);
    }

    // Python's json.dump defaults to ensure_ascii, so a favourite holding a non-BMP character
    // reaches this parser as a surrogate pair and has to come back as the one character it names.
    #[test]
    fn a_surrogate_pair_is_one_character_and_not_two_replacements() {
        let doc = parse(r#"{"favourites":["/home/gm/\ud83d\udcc1 Work"]}"#).expect("parse");
        let first = doc.get("favourites").and_then(Json::as_array).expect("favourites")[0]
            .as_str().expect("string");
        assert_eq!(first, "/home/gm/\u{1F4C1} Work");
        assert!(render(&doc).contains('\u{1F4C1}'), "the rewrite carries the character, not a replacement");
        assert_eq!(parse(&render(&doc)).expect("reparse"), doc);
        // A half with no partner is still the replacement character, which is what the file says.
        assert_eq!(parse(r#"{"a":"\ud83d"}"#).expect("parse").get("a").and_then(Json::as_str), Some("\u{FFFD}"));
        assert_eq!(parse(r#"{"a":"\udcc1x"}"#).expect("parse").get("a").and_then(Json::as_str), Some("\u{FFFD}x"));
        // A high half followed by an escape that is not its partner keeps both, each on its own.
        assert_eq!(parse(r#"{"a":"\ud83d\u0041"}"#).expect("parse").get("a").and_then(Json::as_str), Some("\u{FFFD}A"));
        assert_eq!(parse(r#"{"a":"\ud83dx"}"#).expect("parse").get("a").and_then(Json::as_str), Some("\u{FFFD}x"));
    }

    #[test]
    fn malformed_input_is_an_error_naming_where_it_stopped() {
        for bad in ["", "{", "{\"a\"}", "{\"a\":}", "[1,]", "tru", "{\"a\":1}x", "\"unterminated",
                    "{\"a\":+5}", "{\"a\":.5}", "{\"a\":1.2.3}", "{\"a\":inf}"] {
            assert!(parse(bad).is_err(), "{} should not parse", bad);
        }
    }

    #[test]
    fn a_document_nested_past_the_depth_limit_is_refused_rather_than_recursed() {
        let deep = format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1));
        assert!(parse(&deep).is_err());
        let ok = format!("{}{}", "[".repeat(MAX_DEPTH - 1), "]".repeat(MAX_DEPTH - 1));
        assert!(parse(&ok).is_ok());
    }

    #[test]
    fn a_duplicate_key_keeps_the_last_one_the_way_a_json_reader_does() {
        let v = parse(r#"{"a":1,"a":2}"#).expect("parse");
        assert_eq!(v.as_object().expect("object").len(), 1);
        assert_eq!(v.get("a").and_then(Json::as_f64), Some(2.0));
    }

    #[test]
    fn render_indents_nested_containers_and_ends_with_one_newline() {
        let v = parse(r#"{"o":{"a":[1,2]},"e":{},"l":[]}"#).expect("parse");
        assert_eq!(
            render(&v),
            "{\n  \"o\": {\n    \"a\": [\n      1,\n      2\n    ]\n  },\n  \"e\": {},\n  \"l\": []\n}\n"
        );
    }
}
