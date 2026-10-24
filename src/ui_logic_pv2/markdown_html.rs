//! One forward scan allowlists raw tags, so only sanitized markup reaches the renderer.

use super::markdown_text::{at, find_byte};

#[path = "markdown_html_sanitize.rs"]
mod sanitize;
#[cfg(test)]
pub(super) use sanitize::attr_kept;
pub use sanitize::{sanitize_tag, Sanitized};

/// The private-use marks around a held token's number inside sanitizer output.
pub const TOKEN_OPEN: char = '\u{e002}';
pub const TOKEN_CLOSE: char = '\u{e003}';
pub const MAX_TAG_LENGTH: usize = 4096;
const DISCLOSURE_OPEN: char = '\u{25be}';

/// The document as the passes read it: reserved token marks replaced and every line ending a newline.
pub fn document_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            TOKEN_OPEN | TOKEN_CLOSE => out.push('\u{fffd}'),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            _ => out.push(c),
        }
    }
    out
}

/// A held span's token in sanitizer output, its markup pushed onto tokens.
pub fn hold_token(tokens: &mut Vec<String>, html: String) -> String {
    let token = format!("{TOKEN_OPEN}{}{TOKEN_CLOSE}", tokens.len());
    tokens.push(html);
    token
}

/// Tags whose content is dropped with them: active content and whole-document namespaces a preview never instantiates.
pub fn drops_content(name: &str) -> Option<&'static str> {
    [
        "script", "style", "iframe", "object", "embed", "template", "noscript", "svg", "math",
    ]
    .into_iter()
    .find(|tag| *tag == name)
}

// Tags re-emitted with the attributes below; any other tag is dropped and its content kept.
fn allowed(name: &str) -> bool {
    matches!(
        name,
        "a" | "b"
            | "strong"
            | "i"
            | "em"
            | "u"
            | "s"
            | "del"
            | "strike"
            | "sub"
            | "sup"
            | "kbd"
            | "code"
            | "br"
            | "small"
            | "mark"
            | "span"
            | "p"
            | "div"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "img"
            | "picture"
            | "source"
            | "details"
            | "summary"
            | "table"
            | "thead"
            | "tbody"
            | "tr"
            | "th"
            | "td"
            | "ul"
            | "ol"
            | "li"
            | "blockquote"
            | "pre"
            | "font"
    )
}

// Attributes that survive on any allowed tag.
fn global_attr(name: &str) -> bool {
    matches!(
        name,
        "align" | "alt" | "width" | "height" | "title" | "colspan" | "rowspan"
    )
}

fn void(name: &str) -> bool {
    matches!(name, "br" | "hr" | "img" | "source")
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric()
}

/// The HTML whitespace set: tab, newline, form feed, carriage return and space.
pub fn is_html_space(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

/// What one scan learnt about the text ahead, so dense hostile input pays once.
#[derive(Debug, Default)]
pub struct Dead {
    pub tag_dead: usize,
    pub comment_dead: bool,
    pub tag_close: Option<usize>,
}

/// A raw tag read at text[i] == "<": its text and the index after its ">", or None when none closes it.
pub fn read_tag<'t>(
    text: &'t str,
    i: usize,
    mut dead: Option<&mut Dead>,
) -> Option<(&'t str, usize)> {
    if let Some(d) = dead.as_deref() {
        if i < d.tag_dead {
            return None;
        }
    }
    let cached = dead
        .as_deref()
        .and_then(|d| d.tag_close)
        .filter(|close| *close > i);
    let gt = cached.or_else(|| find_byte(text, b'>', i + 1));
    if let Some(d) = dead.as_deref_mut() {
        if let Some(gt) = gt {
            d.tag_close = Some(gt);
        }
    }
    let Some(gt) = gt else {
        if let Some(d) = dead.as_deref_mut() {
            d.tag_dead = text.len();
        }
        return None;
    };
    if gt - i > MAX_TAG_LENGTH {
        if let Some(d) = dead {
            d.tag_dead = gt - MAX_TAG_LENGTH;
        }
        return None;
    }
    let candidate = &text[i..=gt];
    if !candidate.contains('"') && !candidate.contains('\'') {
        return Some((candidate, gt + 1));
    }
    let mut quote = 0u8;
    for &c in &text.as_bytes()[i + 1..gt] {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        }
    }
    (quote == 0).then_some((candidate, gt + 1))
}

/// One attribute as written: its lowercased name and its value, None for a bare name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub value: Option<String>,
}

fn valid_attr_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    (first.is_ascii_alphabetic() || matches!(first, b'_' | b':'))
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

// " a=b/" keeps the slash in b/; " ==/" names "=" with value "/"; ' a="b"/' sets the self-closing flag.
pub(super) fn scan_attributes(rest: &str) -> (Vec<Attribute>, bool, bool) {
    let bytes = rest.as_bytes();
    let mut i = 0;
    let mut attributes = Vec::new();
    let mut valid = true;
    let mut self_close = false;
    while i < bytes.len() {
        while i < bytes.len() && is_html_space(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        if bytes[i] == b'/' {
            self_close = i == bytes.len() - 1;
            i += 1;
            continue;
        }
        // Before an attribute name, even "=" starts the name; only a later "=" starts its value.
        let first = super::markdown_text::char_at(rest, i).unwrap_or(' ');
        let mut name: String = first.to_lowercase().collect();
        i += first.len_utf8();
        while i < bytes.len() && !is_html_space(bytes[i]) && !matches!(bytes[i], b'=' | b'/' | b'>')
        {
            let c = super::markdown_text::char_at(rest, i).unwrap_or(' ');
            name.extend(c.to_lowercase());
            i += c.len_utf8();
        }
        if !valid_attr_name(&name) {
            valid = false;
        }
        while i < bytes.len() && is_html_space(bytes[i]) {
            i += 1;
        }
        let mut value = None;
        if at(rest, i) == b'=' {
            i += 1;
            while i < bytes.len() && is_html_space(bytes[i]) {
                i += 1;
            }
            let q = at(rest, i);
            if q == b'"' || q == b'\'' {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
                value = Some(rest[start..i.min(bytes.len())].to_string());
                i += 1;
            } else {
                let begin = i;
                while i < bytes.len() && !is_html_space(bytes[i]) && bytes[i] != b'>' {
                    i += 1;
                }
                value = Some(rest[begin..i].to_string());
            }
        }
        attributes.push(Attribute { name, value });
    }
    (attributes, valid, self_close)
}

/// A tag's head: its lowercased name ("" when malformed), whether it closes or self-closes, and its attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagHead {
    pub name: String,
    pub closing: bool,
    pub self_close: bool,
    pub attributes: Vec<Attribute>,
    pub valid_attrs: bool,
}

/// The head of a raw tag: '<img src="pic.png"/>' answers name "img", its attributes and a self-closing flag.
pub fn tag_head(tag: &str) -> TagHead {
    let ascii_edges = tag.as_bytes().first().map_or(true, u8::is_ascii)
        && tag.as_bytes().last().map_or(true, u8::is_ascii);
    // The reader counts UTF-16 units: one skipped before the name, one dropped at the end, a lone surrogate reading as U+FFFD.
    let wide;
    let text = if ascii_edges {
        tag
    } else {
        let units: Vec<u16> = tag.encode_utf16().collect();
        wide = format!(
            "<{}>",
            String::from_utf16_lossy(
                units
                    .get(1..units.len().saturating_sub(1))
                    .unwrap_or_default()
            )
        );
        wide.as_str()
    };
    let bytes = text.as_bytes();
    let mut i = 1.min(bytes.len());
    let closing = at(text, i) == b'/';
    if closing {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && (is_name_char(bytes[i]) || bytes[i] == b'-') {
        i += 1;
    }
    let mut name = text[start..i].to_ascii_lowercase();
    let rest = &text[i..bytes.len().saturating_sub(1).max(i)];
    let slash_first = rest.starts_with('/');
    let mut space_end = usize::from(slash_first);
    while space_end < rest.len() && is_html_space(rest.as_bytes()[space_end]) {
        space_end += 1;
    }
    let valid_head = name
        .as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_lowercase())
        && (rest.is_empty() || is_html_space(rest.as_bytes()[0]) || slash_first)
        && (!closing || (!slash_first && space_end == rest.len()))
        && (!slash_first || space_end == rest.len());
    // Malformed dropped openers still suppress their bodies; malformed closers cannot end a dropped body.
    if !valid_head && (closing || drops_content(&name).is_none()) {
        name.clear();
    }
    let (attributes, valid_attrs, self_close) = scan_attributes(rest);
    TagHead {
        name,
        closing,
        self_close: valid_head && valid_attrs && self_close,
        attributes,
        valid_attrs,
    }
}
