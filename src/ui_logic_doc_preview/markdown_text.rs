//! Text readers shared by the Markdown passes: the whitespace set, trim and byte probes the parser decides with.

/// The whitespace `\s` and `trim` know: ASCII blanks, the no-break and Unicode spaces, the line separators and U+FEFF.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// The text without the whitespace at either end.
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// True when the text holds nothing but whitespace.
pub fn blank(text: &str) -> bool {
    text.chars().all(is_space)
}

/// The byte at an index, or 0 past the end, so a probe past the text matches no character.
pub fn at(text: &str, i: usize) -> u8 {
    text.as_bytes().get(i).copied().unwrap_or(0)
}

/// The character that starts at a byte index, if any.
pub fn char_at(text: &str, i: usize) -> Option<char> {
    text.get(i..).and_then(|rest| rest.chars().next())
}

/// The character that ends at a byte index, if any.
pub fn char_before(text: &str, i: usize) -> Option<char> {
    text.get(..i).and_then(|head| head.chars().next_back())
}

/// The index of a byte at or after from, the way indexOf reads a one-character needle.
pub fn find_byte(text: &str, byte: u8, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .iter()
        .position(|b| *b == byte)
        .map(|at| at + from)
}

/// The index of a needle at or after from.
pub fn find_str(text: &str, needle: &str, from: usize) -> Option<usize> {
    if from > text.len() {
        return None;
    }
    let bytes = text.as_bytes();
    let wanted = needle.as_bytes();
    if wanted.is_empty() {
        return Some(from);
    }
    bytes[from..]
        .windows(wanted.len())
        .position(|w| w == wanted)
        .map(|at| at + from)
}

/// The last index of a byte at or before from, the way lastIndexOf reads a one-character needle.
pub fn rfind_byte(text: &str, byte: u8, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let top = from.min(bytes.len() - 1);
    bytes[..=top].iter().rposition(|b| *b == byte)
}

/// A slice that clamps both ends to the text and to character boundaries, as slice does with indices past the end.
pub fn slice(text: &str, from: usize, to: usize) -> &str {
    let to = floor(text, to.min(text.len()));
    let from = floor(text, from.min(to));
    &text[from..to]
}

/// The largest character boundary at or below an index.
pub fn floor(text: &str, mut i: usize) -> usize {
    i = i.min(text.len());
    while !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// True when every byte of the text is an ASCII digit and there is at least one.
pub fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// True for an ASCII letter, digit or underscore, the `\w` class.
pub fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// A scheme as `[a-zA-Z][a-zA-Z0-9+.-]*:` at the start of the text: its length with the colon.
pub fn scheme_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if !bytes.first()?.is_ascii_alphabetic() {
        return None;
    }
    let mut i = 1;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'+' | b'.' | b'-'))
    {
        i += 1;
    }
    (bytes.get(i) == Some(&b':')).then_some(i + 1)
}

/// True for a hexadecimal colour the parser accepts as chrome or ink: `#rrggbb` or `#rrggbbaa`.
pub fn is_colour(text: &str) -> bool {
    let bytes = text.as_bytes();
    (bytes.len() == 7 || bytes.len() == 9)
        && bytes[0] == b'#'
        && bytes[1..].iter().all(|b| b.is_ascii_hexdigit())
}

/// The lines of a text split at each newline, an empty text answering one empty line as split does.
pub fn lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// A count with a comma between each group of three digits.
pub fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A template with each `%1` to `%9` filled from the values, a missing value leaving its token as written.
pub fn fill(template: &str, values: &[&str]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            if let Some(&d) = chars.peek() {
                if ('1'..='9').contains(&d) {
                    chars.next();
                    let at = d as usize - '1' as usize;
                    match values.get(at) {
                        Some(value) => out.push_str(value),
                        None => {
                            out.push('%');
                            out.push(d);
                        }
                    }
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}
