// One whole JSON document in and out, which src/json.rs deliberately is not: it scans one wire line.
use crate::json::escape;
use crate::jsonstring::parse_string;

// A hand-edited state file is an input, so nesting is bounded rather than recursed until the stack ends.
pub const MAX_DEPTH: usize = 32;

const INDENT: &str = "  ";

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
    let text = std::str::from_utf8(&bytes[start..*at]).map_err(|_| format!("a number that is not UTF-8 at byte {}", start))?;
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

    // Rust's f64 parse takes literals JSON does not, and Json::Num keeps the literal, so render
    // writes it straight back and the window's own JSON.parse refuses the file the settle wrote.
    #[test]
    fn a_number_only_rust_takes_is_refused_rather_than_written_back() {
        let mut written_back = Vec::new();
        for bad in ["1.", "1.e5", "5.e3", "0192", "-0192", "01", "00", "-.5", "+1", ".5", "1e", "0x1"] {
            let doc = format!("{{\"a\":{}}}", bad);
            if let Ok(v) = parse(&doc) {
                written_back.push(format!("{} -> {}", bad, render(&v).replace('\n', "")));
            }
        }
        assert!(written_back.is_empty(), "written back verbatim: {:?}", written_back);
        for good in ["0", "-0", "192", "-12", "1.0", "0.5", "1e5", "1E5", "1e+5", "1.5e-3"] {
            let v = parse(&format!("{{\"a\":{}}}", good)).unwrap_or_else(|e| panic!("{} must parse: {}", good, e));
            assert_eq!(render(&v), format!("{{\n  \"a\": {}\n}}\n", good), "the literal is written back as it was read");
        }
    }

    #[test]
    fn strings_survive_a_round_trip_with_every_escape_in_them() {
        let v = parse(r#"{"k":"a\"b\\c\nd\teAé\/f"}"#).expect("parse");
        assert_eq!(v.get("k").and_then(Json::as_str), Some("a\"b\\c\nd\te\u{41}\u{e9}/f"));
        assert_eq!(parse(&render(&v)).expect("reparse"), v);
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
