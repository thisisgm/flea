//! The readers of a link's pieces: a destination and title, a reference label, an autolink and a bare address.

use super::markdown_inline::is_blank_byte;
use super::markdown_text::{at, is_word};

const TARGET_SCAN_LIMIT: usize = 8192;
const LABEL_SCAN_LIMIT: usize = 1000;
const MIN_BARELINK_LENGTH: usize = 9;

// "\n  x" read at 0 answers 4, past the line ending and the indent; "\n \n x" answers None, a blank line.
fn skip_blanks(text: &str, mut j: usize, cap: usize) -> Option<usize> {
    let mut ending = false;
    while j < text.len() && j < cap {
        let c = at(text, j);
        if c == b'\r' && at(text, j + 1) == b'\n' {
            j += 1;
        } else if !is_blank_byte(c) {
            break;
        }
        if c == b'\n' || c == b'\r' {
            if ending {
                return None;
            }
            ending = true;
        }
        j += 1;
    }
    Some(j)
}

/// The ">" that closes an angle destination read from from, or None at a line ending or, when strict, a "<".
pub fn angle_close(text: &str, from: usize, limit: usize, strict: bool) -> Option<usize> {
    let mut j = from;
    while j < text.len() && j < limit {
        let c = at(text, j);
        if c == b'>' {
            return Some(j);
        }
        if c == b'\n' || c == b'\r' || (strict && c == b'<') {
            return None;
        }
        // A backslash before a line ending is no escape, so the destination ends there unclosed.
        if c == b'\\' && matches!(at(text, j + 1), b'\n' | b'\r') {
            return None;
        }
        j += if c == b'\\' { 2 } else { 1 };
    }
    None
}

/// A destination and optional title after a label's "]": the URL as written and the index past ")".
pub fn read_inline_target(text: &str, i: usize) -> Option<(String, usize)> {
    if at(text, i) != b'(' {
        return None;
    }
    // Targets past the scan limit read as literal text: no real target runs that long, and an unbounded walk is quadratic.
    let cap = i + TARGET_SCAN_LIMIT;
    let mut j = skip_blanks(text, i + 1, cap)?;
    let url;
    if at(text, j) == b'<' {
        j += 1;
        let start = j;
        j = angle_close(text, j, cap, true)?;
        url = &text[start..j];
        j += 1;
    } else {
        let mut depth = 0usize;
        let begin = j;
        while j < text.len() && j < cap {
            let c = at(text, j);
            if c == b'\n' || c == b'\r' || ((c == b' ' || c == b'\t') && depth == 0) {
                break;
            }
            if c == b'\\' {
                if matches!(at(text, j + 1), b'\n' | b'\r') {
                    return None;
                }
                j += 2;
                continue;
            }
            if c == b'(' {
                depth += 1;
            } else if c == b')' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            j += 1;
        }
        if j >= cap {
            return None;
        }
        j = j.min(text.len());
        url = &text[begin..super::markdown_text::floor(text, j)];
    }
    let gap = j;
    j = skip_blanks(text, j, cap)?;
    let opener = at(text, j);
    if j > gap && matches!(opener, b'"' | b'\'' | b'(') {
        let close = if opener == b'(' { b')' } else { opener };
        j += 1;
        while j < text.len() && j < cap {
            let c = at(text, j);
            if c == close {
                break;
            }
            if c == b'\n' || c == b'\r' {
                j = skip_blanks(text, j, cap)?;
                continue;
            }
            let escaped = c == b'\\' && !matches!(at(text, j + 1), b'\n' | b'\r');
            j += if escaped { 2 } else { 1 };
        }
        if j >= text.len() || j >= cap || at(text, j) != close {
            return None;
        }
        j = skip_blanks(text, j + 1, cap)?;
    }
    if j >= cap || at(text, j) != b')' {
        return None;
    }
    Some((url.to_string(), j + 1))
}

/// A "[label]" or a collapsed "[label][]": the label as written and the index past its "]".
pub fn read_label_ref(text: &str, i: usize) -> Option<(&str, usize)> {
    if at(text, i) != b'[' {
        return None;
    }
    let mut j = i + 1;
    while j < text.len() && j - i < LABEL_SCAN_LIMIT {
        let c = at(text, j);
        if c == b']' {
            break;
        }
        if c == b'[' {
            return None;
        }
        if c == b'\\' {
            j += 2;
            continue;
        }
        j += 1;
    }
    if j >= text.len() || j - i >= LABEL_SCAN_LIMIT {
        return None;
    }
    Some((&text[i + 1..j], j + 1))
}

/// A label as the reference map keys it: blanks folded to one space, trimmed, lowercased, sharp s as "ss".
pub fn normalize_label(label: &str) -> String {
    let mut folded = String::with_capacity(label.len());
    let mut blank = false;
    for c in label.chars() {
        if matches!(c, '\t' | '\n' | ' ') {
            blank = true;
            continue;
        }
        if blank {
            folded.push(' ');
            blank = false;
        }
        folded.push(c);
    }
    if blank {
        folded.push(' ');
    }
    let trimmed = folded.strip_prefix(' ').unwrap_or(&folded);
    let trimmed = trimmed.strip_suffix(' ').unwrap_or(trimmed);
    trimmed.to_lowercase().replace('\u{df}', "ss")
}

fn is_mail_local(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&b)
}

// CommonMark's email address: its local part set and dot-separated host labels.
fn is_mail_address(text: &str) -> bool {
    let Some((local, host)) = text.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.bytes().all(is_mail_local)
        && host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// An autolink read at text[i] == "<": its shown URL, its href (mailto: added for an address) and the index past ">".
pub fn read_autolink(text: &str, i: usize) -> Option<(String, String, usize)> {
    let mut j = i + 1;
    while j < text.len()
        && j - i < TARGET_SCAN_LIMIT
        && at(text, j) != b'>'
        && !is_blank_byte(at(text, j))
    {
        j += 1;
    }
    if j >= text.len() || j - i >= TARGET_SCAN_LIMIT || at(text, j) != b'>' {
        return None;
    }
    let inner = &text[i + 1..j];
    if let Some(len) = super::markdown_text::scheme_len(inner) {
        let name = len - 1;
        if (2..=32).contains(&name) && !inner[len..].contains(['<', '>']) {
            return Some((inner.to_string(), inner.to_string(), j + 1));
        }
    }
    if is_mail_address(inner) {
        return Some((inner.to_string(), format!("mailto:{inner}"), j + 1));
    }
    None
}

/// A bare address read at i: the shown URL, its href (http:// added for "www.") and the index after it, trailing punctuation left out.
pub fn read_barelink(text: &str, i: usize) -> Option<(String, String, usize)> {
    let rest = &text[i..];
    let http = rest.starts_with("http://") || rest.starts_with("https://");
    if !http {
        if !rest.starts_with("www.") {
            return None;
        }
        let before = if i > 0 { at(text, i - 1) } else { b' ' };
        if is_word(before) || before == b'/' || before == b'@' {
            return None;
        }
    }
    let mut j = i;
    while j < text.len() && !is_blank_byte(at(text, j)) && at(text, j) != b'<' {
        j += 1;
    }
    let mut parens: i64 = 0;
    for &b in &text.as_bytes()[i..j] {
        parens += match b {
            b'(' => 1,
            b')' => -1,
            _ => 0,
        };
    }
    while j > i {
        let tail = at(text, j - 1);
        if !b"?!.,;:".contains(&tail) && !(tail == b')' && parens < 0) {
            break;
        }
        if tail == b')' {
            parens += 1;
        }
        j -= 1;
    }
    let url = &text[i..j];
    if url.chars().count() < MIN_BARELINK_LENGTH {
        return None;
    }
    let href = if http {
        url.to_string()
    } else {
        format!("http://{url}")
    };
    Some((url.to_string(), href, j))
}
