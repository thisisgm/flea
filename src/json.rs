// The wire's JSON: read one named field out of one line, and escape one string into one.

// The short escapes are matched first, so this is the boundary for all the rest.
const FIRST_PRINTABLE: u32 = 0x20;

// A name may hold any JSON metacharacter, so every one must survive intact.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < FIRST_PRINTABLE => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

// Skips any "<key>": that falls inside a string value.
fn value_start(line: &str, key: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let needle = format!("\"{}\":", key);
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;
    while i < bytes.len() {
        let c = bytes[i];
        if escaped {
            escaped = false;
        } else if c == b'\\' && in_string {
            escaped = true;
        } else if c == b'"' {
            if !in_string && line[i..].starts_with(&needle) {
                return Some(i + needle.len());
            }
            in_string = !in_string;
        }
        i += 1;
    }
    None
}

// Sample input: "/tmp/two\nlines" or "café", the value only, quotes included.
pub fn field_str(line: &str, key: &str) -> Option<String> {
    let start = value_start(line, key)?;
    let rest = line[start..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = rest[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('b') => out.push('\u{8}'),
                Some('f') => out.push('\u{c}'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        Some(u) => out.push(u),
                        None => return None,
                    }
                }
                Some(other) => out.push(other),
                None => return None,
            },
            c => out.push(c),
        }
    }
    None
}

pub fn field_usize(line: &str, key: &str) -> Option<usize> {
    let start = value_start(line, key)?;
    let rest = line[start..].trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

// Sample input: "rows":[2,17,140]; a missing, empty or malformed array is an empty vector.
pub fn field_usize_array(line: &str, key: &str) -> Vec<usize> {
    let rest = value_start(line, key).map(|s| line[s..].trim_start()).unwrap_or("");
    let body = rest.strip_prefix('[').and_then(|r| r.split(']').next()).unwrap_or("");
    body.split(',').filter_map(|p| p.trim().parse().ok()).collect()
}

// Sample input: "paths":["/home/gm/a.txt","/home/gm/b, [odd].txt"]; a path may hold a comma or a
// bracket, so the scan tracks quotes and escapes instead of splitting on either character.
pub fn field_str_array(line: &str, key: &str) -> Vec<String> {
    let rest = match value_start(line, key) {
        Some(s) => line[s..].trim_start(),
        None => return Vec::new(),
    };
    let body = match rest.strip_prefix('[') {
        Some(b) => b,
        None => return Vec::new(),
    };
    let mut out = Vec::new();
    // Only ASCII quote and backslash end a step, and a UTF-8 continuation byte is never either, so every index here is a char boundary.
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b']' => return out,
            b'"' => {
                let close = match closing_quote(bytes, i) {
                    Some(c) => c,
                    // An unterminated element ends the array: whatever was already read stands.
                    None => return out,
                };
                // field_str already decodes one quoted value with its escapes, so an element is read through it.
                if let Some(v) = field_str(&format!("\"x\":{}", &body[i..=close]), "x") {
                    out.push(v);
                }
                i = close + 1;
            }
            _ => i += 1,
        }
    }
    out
}

// The index of the quote that ends the element opening at `open`, stepping over an escaped quote rather than stopping on it.
fn closing_quote(bytes: &[u8], open: usize) -> Option<usize> {
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

pub fn field_bool(line: &str, key: &str) -> bool {
    match value_start(line, key) {
        Some(start) => line[start..].trim_start().starts_with("true"),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_the_characters_json_forbids() {
        assert_eq!(escape("plain.txt"), "plain.txt");
        assert_eq!(escape("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape("back\\slash"), "back\\\\slash");
        assert_eq!(escape("two\nlines"), "two\\nlines");
        assert_eq!(escape("tab\there"), "tab\\there");
        assert_eq!(escape("cr\rhere"), "cr\\rhere");
        assert_eq!(escape("bell\u{7}"), "bell\\u0007");
        assert_eq!(escape("unit\u{1f}sep"), "unit\\u001fsep");
        assert_eq!(escape("a b"), "a b");
    }

    #[test]
    fn leaves_unicode_alone() {
        assert_eq!(escape("caf\u{e9} \u{2615}"), "caf\u{e9} \u{2615}");
    }

    #[test]
    fn a_value_containing_json_syntax_survives_the_scan() {
        let line = r#"{"c":"list","path":"/tmp/a\"b,\"c\":\"d","first":10}"#;
        assert_eq!(field_str(line, "path").as_deref(), Some("/tmp/a\"b,\"c\":\"d"));
    }

    #[test]
    fn a_key_injected_inside_a_value_does_not_win() {
        let line = r#"{"c":"list","path":"/tmp/x\"y","first":10}"#;
        assert_eq!(field_str(line, "path").as_deref(), Some("/tmp/x\"y"));
        assert_eq!(field_usize(line, "first"), Some(10));
    }

    #[test]
    fn a_value_containing_a_newline_survives_the_scan() {
        let line = r#"{"c":"list","path":"/tmp/two\nlines","first":1}"#;
        assert_eq!(field_str(line, "path").as_deref(), Some("/tmp/two\nlines"));
    }

    #[test]
    fn every_json_escape_decodes_to_its_character() {
        let line = r#"{"c":"list","path":"a\"b\\c\/d\be\ff\ng\rh\tiAj","first":1}"#;
        assert_eq!(
            field_str(line, "path").as_deref(),
            Some("a\"b\\c/d\u{8}e\u{c}f\ng\rh\tiAj")
        );
    }

    #[test]
    fn a_missing_or_unquoted_field_is_none_rather_than_a_panic() {
        assert_eq!(field_str(r#"{"c":"list"}"#, "path"), None);
        assert_eq!(field_str(r#"{"c":"list","path":7}"#, "path"), None);
        assert_eq!(field_usize(r#"{"c":"window"}"#, "start"), None);
        assert_eq!(field_usize(r#"{"c":"window","start":"x"}"#, "start"), None);
        assert!(!field_bool(r#"{"c":"sort"}"#, "desc"));
        assert!(!field_bool(r#"{"c":"sort","desc":truthy}"#, "desc"));
        assert!(field_bool(r#"{"c":"sort","desc":true}"#, "desc"));
    }

    #[test]
    fn a_usize_array_keeps_the_indices_it_can_read_and_drops_the_rest() {
        assert_eq!(field_usize_array(r#"{"rows":[2,17,140]}"#, "rows"), vec![2, 17, 140]);
        // Order and repeats are the client's business, so both survive the scan unchanged.
        assert_eq!(field_usize_array(r#"{"rows":[9,0,9,0]}"#, "rows"), vec![9, 0, 9, 0]);
        assert_eq!(field_usize_array(r#"{"rows":[ 3 , 4 ]}"#, "rows"), vec![3, 4]);
    }

    #[test]
    fn a_malformed_usize_array_is_empty_rather_than_a_panic() {
        // A missing key, a key that is not an array, and an empty one.
        assert!(field_usize_array(r#"{"c":"thumb"}"#, "rows").is_empty());
        assert!(field_usize_array(r#"{"rows":7}"#, "rows").is_empty());
        assert!(field_usize_array(r#"{"rows":[]}"#, "rows").is_empty());
        // A truncated line yields what it could read, which is safe because every index is clamped to the listing.
        assert_eq!(field_usize_array(r#"{"rows":[1,2"#, "rows"), vec![1, 2]);
        // A negative index, a float and a name are each dropped while their neighbours survive.
        assert_eq!(field_usize_array(r#"{"rows":[-1,5]}"#, "rows"), vec![5]);
        assert_eq!(field_usize_array(r#"{"rows":[1.5,6]}"#, "rows"), vec![6]);
        assert_eq!(field_usize_array(r#"{"rows":[nope,7]}"#, "rows"), vec![7]);
        // Wider than u64, so the parse fails and the row beside it still lands.
        assert_eq!(field_usize_array(r#"{"rows":[99999999999999999999999999,8]}"#, "rows"), vec![8]);
        assert_eq!(field_usize_array(r#"{"rows":[18446744073709551615]}"#, "rows"), vec![usize::MAX]);
    }
    #[test]
    fn a_path_array_survives_a_comma_and_a_bracket_inside_a_path() {
        let line = r#"{"c":"trash","paths":["/home/gm/a.txt","/home/gm/b, [odd].txt"]}"#;
        assert_eq!(
            field_str_array(line, "paths"),
            vec!["/home/gm/a.txt".to_string(), "/home/gm/b, [odd].txt".to_string()],
            "splitting on the comma or the bracket would tear this second path in half"
        );
    }

    #[test]
    fn a_path_array_decodes_the_escapes_field_str_already_handles() {
        let line = r#"{"paths":["/tmp/say \"hi\".txt","/tmp/two\nlines","/tmp/caf\u00e9"]}"#;
        assert_eq!(
            field_str_array(line, "paths"),
            vec![
                "/tmp/say \"hi\".txt".to_string(),
                "/tmp/two\nlines".to_string(),
                "/tmp/caf\u{e9}".to_string()
            ]
        );
    }

    #[test]
    fn a_missing_empty_or_malformed_path_array_is_an_empty_vector_and_never_a_panic() {
        assert!(field_str_array(r#"{"c":"trash"}"#, "paths").is_empty());
        assert!(field_str_array(r#"{"paths":[]}"#, "paths").is_empty());
        assert!(field_str_array(r#"{"paths":"not an array"}"#, "paths").is_empty());
        assert!(field_str_array(r#"{"paths":[123,456]}"#, "paths").is_empty());
        // An element with no closing quote ends the array rather than running off the line.
        assert_eq!(field_str_array(r#"{"paths":["/a","/unterminated"#, "paths"), vec!["/a".to_string()]);
        assert!(field_str_array("", "paths").is_empty());
    }

    #[test]
    fn a_path_array_stops_at_its_own_bracket_and_ignores_what_follows() {
        let line = r#"{"paths":["/a"],"dest":"/should not be read as a path"}"#;
        assert_eq!(field_str_array(line, "paths"), vec!["/a".to_string()]);
    }
}
