// One JSON string in and one Rust String out: the escapes, and UTF-16's surrogate pairs, which
// only src/jsondoc.rs's whole-document reader needs. The other direction is src/json.rs's escape.

// UTF-16's surrogate halves, which are not scalar values and only mean anything as a pair.
const HIGH_FIRST: u32 = 0xd800;
const HIGH_LAST: u32 = 0xdbff;
const LOW_FIRST: u32 = 0xdc00;
const LOW_LAST: u32 = 0xdfff;

pub fn parse_string(bytes: &[u8], at: &mut usize) -> Result<String, String> {
    if bytes.get(*at) != Some(&b'"') {
        return Err(format!("a string was expected at byte {}", at));
    }
    let opened = *at;
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
    Err(format!("a string opened at byte {} ran to the end of the document", opened))
}

fn unescape(bytes: &[u8], at: &mut usize) -> Result<char, String> {
    let code = *bytes.get(*at).ok_or_else(|| format!("an escape ran off the end at byte {}", *at))?;
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
// serializer writes U+1F4C1 as; a surrogate with no partner becomes the replacement character.
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
    let digits = bytes.get(*at..end).ok_or_else(|| format!("a short \\u escape at byte {}", *at))?;
    let text = std::str::from_utf8(digits).map_err(|_| format!("a \\u escape that is not hex at byte {}", *at))?;
    let point = u32::from_str_radix(text, 16).map_err(|_| format!("a \\u escape that is not hex at byte {}", *at))?;
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

#[cfg(test)]
mod tests {
    use crate::jsondoc::{parse, render, Json};

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
}
