// The text a preview shows from a file's bytes: UTF-8 with its byte order mark dropped, UTF-16 by its byte order mark, and Windows-1252 for text that holds no NUL and fewer UTF-8 characters than stray bytes.
use std::borrow::Cow;

// The characters Windows-1252 puts at 0x80 to 0x9F; its five unassigned bytes keep their own C1 code points, as WHATWG decodes them.
const HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

// Sample input: b"\xef\xbb\xbf# t" gives "# t", b"\xff\xfeh\0i\0" gives "hi", b"caf\xe9" gives "café"; bytes with a NUL and no UTF-16 mark stay lossy UTF-8, so a binary check still sees the NUL.
pub fn decode(bytes: &[u8]) -> Cow<'_, str> {
    if let Some(rest) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        // Owned, so a caller can tell the text changed from its bytes.
        return Cow::Owned(String::from_utf8_lossy(rest).into_owned());
    }
    if let Some(text) = utf16(bytes) {
        return Cow::Owned(text);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) if !bytes.contains(&0) && !mostly_utf8(bytes) => {
            Cow::Owned(bytes.iter().map(|&b| windows_1252(b)).collect())
        }
        Err(_) => String::from_utf8_lossy(bytes),
    }
}

// At least as many valid multi-byte UTF-8 characters as invalid sequences, a tie going to UTF-8 since a chance pair is rarer than a stray byte.
fn mostly_utf8(bytes: &[u8]) -> bool {
    let (mut valid, mut invalid) = (0usize, 0usize);
    let mut rest = bytes;
    loop {
        let (good, skip) = match std::str::from_utf8(rest) {
            Ok(text) => (text, None),
            Err(error) => (
                std::str::from_utf8(&rest[..error.valid_up_to()]).unwrap_or_default(),
                Some(error.valid_up_to() + error.error_len().unwrap_or(1)),
            ),
        };
        valid += good.chars().filter(|c| !c.is_ascii()).count();
        match skip {
            Some(skip) => {
                invalid += 1;
                if skip >= rest.len() {
                    return valid >= invalid;
                }
                rest = &rest[skip..];
            }
            None => return valid >= invalid,
        }
    }
}

// decode's text as UTF-8 bytes, handing back the bytes themselves when they already were.
pub fn into_utf8(bytes: Vec<u8>) -> Vec<u8> {
    let decoded = match decode(&bytes) {
        Cow::Owned(text) => Some(text.into_bytes()),
        Cow::Borrowed(_) => None,
    };
    decoded.unwrap_or(bytes)
}

// Whether a file's first bytes read as text: a UTF-16 mark, or no NUL and either UTF-8 (a character the cut went through may be incomplete) or single-byte text with no control other than whitespace and escape.
// Sample input: b"plain\n" and b"caf\xe9\n" are text, b"\x00\x01" and b"\x01\x02\x03\xff" are not.
pub fn looks_text(head: &[u8], cut: bool) -> bool {
    if head.starts_with(b"\xff\xfe") || head.starts_with(b"\xfe\xff") {
        return utf16(head).is_some_and(|text| !text.contains('\0'));
    }
    if head.contains(&0) {
        return false;
    }
    match std::str::from_utf8(head) {
        Ok(_) => true,
        Err(error) if error.error_len().is_none() && cut => true,
        Err(_) => head
            .iter()
            .all(|&b| b >= 0x20 || matches!(b, b'\t' | b'\n' | b'\r' | 0x0c | 0x1b)),
    }
}

fn utf16(bytes: &[u8]) -> Option<String> {
    let (rest, big) = match bytes {
        [0xff, 0xfe, rest @ ..] => (rest, false),
        [0xfe, 0xff, rest @ ..] => (rest, true),
        _ => return None,
    };
    let units = rest.chunks_exact(2).map(|pair| {
        if big {
            u16::from_be_bytes([pair[0], pair[1]])
        } else {
            u16::from_le_bytes([pair[0], pair[1]])
        }
    });
    Some(
        char::decode_utf16(units)
            .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
            .collect(),
    )
}

fn windows_1252(byte: u8) -> char {
    match byte {
        0x80..=0x9f => HIGH[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

#[cfg(test)]
#[path = "textdecode_tests.rs"]
mod tests;
