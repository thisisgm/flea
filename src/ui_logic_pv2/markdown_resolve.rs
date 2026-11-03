//! Bracket pairs into held local images, escaped placeholders, safe links or literal text; tags and bare addresses in a scan.

use super::markdown_entity::decode_references;
use super::markdown_html::{read_tag, sanitize_tag, tag_head, Dead, TOKEN_CLOSE, TOKEN_OPEN};
use super::markdown_inline::{code_html, escape_html_text, link_html, SpanKind};
use super::markdown_link::{
    normalize_label, read_autolink, read_barelink, read_inline_target, read_label_ref,
};
use super::markdown_piece::Piece;
use super::markdown_refs::skip_drop_content;
use super::markdown_text::{at, find_str};
use super::markdown_url::{classify_image, stripped_target, target_allowed, Picture};
use super::Ctx;
use std::collections::HashMap;

const MAX_STYLED_SPANS: usize = 1024;

/// A destination as the markdown the renderer reads back to exactly this text.
pub fn markdown_destination(target: &str) -> String {
    let mut out = String::with_capacity(target.len());
    for c in target.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '\\' => out.push_str("\\\\"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// True for a link that opens in the default application: an allowed target that names its scheme.
pub fn is_external_link(url: &str) -> bool {
    super::markdown_url::is_external_link(url)
}

/// The destination after a label's "]" at j: an inline target, a full or collapsed reference, or a bare label (raw).
pub fn read_destination(
    body: &str,
    j: usize,
    raw: &str,
    defs: &HashMap<String, String>,
) -> Option<(String, usize)> {
    let (url, end) = read_raw_destination(body, j, raw, defs)?;
    Some((decode_references(&url), end))
}

fn read_raw_destination(
    body: &str,
    j: usize,
    raw: &str,
    defs: &HashMap<String, String>,
) -> Option<(String, usize)> {
    if at(body, j) == b'(' {
        if let Some(found) = read_inline_target(body, j) {
            return Some(found);
        }
    }
    if at(body, j) == b'[' {
        let (label, end) = read_label_ref(body, j)?;
        let label = if label.is_empty() { raw } else { label };
        if label.is_empty() {
            return None;
        }
        return defs
            .get(&normalize_label(label))
            .map(|url| (url.clone(), end));
    }
    if raw.is_empty() {
        return None;
    }
    defs.get(&normalize_label(raw)).map(|url| (url.clone(), j))
}

/// A label's text alone: tags go, numeric references turn back into characters.
pub fn plain_text(html: &str) -> String {
    let mut stripped = String::with_capacity(html.len());
    let mut i = 0;
    let mut plain = 0;
    while i < html.len() {
        if at(html, i) == b'<' {
            if let Some(close) = super::markdown_text::find_byte(html, b'>', i) {
                stripped.push_str(&html[plain..i]);
                i = close + 1;
                plain = i;
                continue;
            }
        }
        i += 1;
    }
    stripped.push_str(&html[plain..]);
    let mut out = String::with_capacity(stripped.len());
    let (mut i, mut plain) = (0, 0);
    while i < stripped.len() {
        if at(&stripped, i) == b'&' && at(&stripped, i + 1) == b'#' {
            let digits = stripped[i + 2..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
            if digits > 0 && at(&stripped, i + 2 + digits) == b';' {
                out.push_str(&stripped[plain..i]);
                let code: u64 = stripped[i + 2..i + 2 + digits].parse().unwrap_or(0xfffd);
                out.push(char::from_u32((code & 0xffff) as u32).unwrap_or('\u{fffd}'));
                i += 3 + digits;
                plain = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&stripped[plain..]);
    out
}

/// What resolving a pair needs from its scan: the label's markup (None when there is none) and the token table.
pub trait PairHost {
    fn label(&mut self) -> Option<String>;
    fn tokens(&mut self) -> &mut Vec<String>;
}

fn push_token(host: &mut dyn PairHost, html: String) -> Piece {
    let tokens = host.tokens();
    tokens.push(html);
    Piece::Token(tokens.len() - 1)
}

// A backslash before punctuation is consumed: the alt and the label show the punctuation, never the escape.
fn unescaped(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if super::markdown_escape::is_ascii_punct(next as u32) {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

/// One bracket pair as text or a token: raw is its label as written, target its destination.
pub fn resolve_pair(
    raw: &str,
    target: &str,
    bang: bool,
    ctx: &Ctx,
    host: &mut dyn PairHost,
) -> Piece {
    let mut clean = unescaped(raw);
    if bang {
        if let Some(label) = host.label() {
            clean = plain_text(&label);
        }
        return match classify_image(target, ctx.dir) {
            Picture::Remote(host_name) => Piece::Text(format!(
                "\n\n{}\n\n",
                ctx.placeholder(&escape_html_text(&host_name))
            )),
            Picture::Local(url) => {
                let html = format!(
                    "![{}]({})",
                    escape_html_text(&clean),
                    markdown_destination(&url)
                );
                push_token(host, html)
            }
            Picture::Dropped => Piece::Text(escape_html_text(&clean)),
        };
    }
    let target = stripped_target(target);
    // Only http, https, mailto, relative and #anchor targets link, checked through every decoding.
    if !target_allowed(&target) {
        return Piece::Text(escape_html_text(&format!("[{clean}]({target})")));
    }
    let label = host.label();
    let markup = label.is_some();
    let shown = label.unwrap_or(clean.clone());
    // A linked image keeps its own link syntax, so its alt text is not drawn beside it.
    let html = if markup && shown.contains("![") {
        Some(format!("[{shown}]({})", markdown_destination(&target)))
    } else {
        link_html(&shown, &target, ctx.ink, markup)
    };
    match html {
        Some(html) => push_token(host, html),
        None => Piece::Text(escape_html_text(&format!("[{clean}]({target})"))),
    }
}

/// Pushes sanitizer output, its held-span marks as token pieces.
pub fn push_emitted(out: &mut Vec<Piece>, tokens_len: usize, emit: String) {
    if !emit.contains(TOKEN_OPEN) {
        out.push(Piece::Text(emit));
        return;
    }
    let mut plain = String::new();
    let mut chars = emit.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != TOKEN_OPEN {
            plain.push(c);
            continue;
        }
        let rest = &emit[i + c.len_utf8()..];
        let close = rest.find(TOKEN_CLOSE);
        let index = close.and_then(|close| rest[..close].parse::<usize>().ok());
        match (close, index) {
            (Some(close), Some(index)) if index < tokens_len => {
                if !plain.is_empty() {
                    out.push(Piece::Text(std::mem::take(&mut plain)));
                }
                out.push(Piece::Token(index));
                let skip = rest[..close].chars().count() + 1;
                for _ in 0..skip {
                    chars.next();
                }
            }
            _ => plain.push(c),
        }
    }
    if !plain.is_empty() {
        out.push(Piece::Text(plain));
    }
}

/// One held span's chip markup, cached by content so a dense document builds each once.
pub fn styled_span(
    kind: SpanKind,
    content: &str,
    chrome: &str,
    cache: &mut HashMap<(SpanKind, String), String>,
) -> String {
    if let Some(held) = cache.get(&(kind, content.to_string())) {
        return held.clone();
    }
    let held = code_html(content, chrome, kind).unwrap_or_default();
    if cache.len() < MAX_STYLED_SPANS {
        cache.insert((kind, content.to_string()), held.clone());
    }
    held
}

/// Reads a "<" at body[i]: an autolink, a comment, a declaration or a tag; answers the index after what it read.
#[allow(clippy::too_many_arguments)]
pub fn parse_angle(
    body: &str,
    i: usize,
    ctx: &Ctx,
    style_links: bool,
    dead: &mut Dead,
    tokens: &mut Vec<String>,
    out: &mut Vec<Piece>,
    inked: &mut usize,
) -> usize {
    if let Some((url, href, end)) =
        read_autolink(body, i).filter(|(_, href, _)| target_allowed(href))
    {
        let html = if style_links {
            link_html(&url, &href, ctx.ink, false)
        } else {
            None
        };
        match html {
            Some(html) => {
                tokens.push(html);
                out.push(Piece::Token(tokens.len() - 1));
            }
            None => out.push(Piece::Text(escape_html_text(&url))),
        }
        return end;
    }
    if body[i..].starts_with("<!--") {
        if dead.comment_dead {
            out.push(Piece::Lit("&#60;"));
            return i + 1;
        }
        return match find_str(body, "-->", i + 4) {
            Some(close) => close + 3,
            None => {
                dead.comment_dead = true;
                out.push(Piece::Lit("&#60;"));
                i + 1
            }
        };
    }
    if matches!(at(body, i + 1), b'!' | b'?') {
        return match read_tag(body, i, Some(dead)) {
            Some((_, end)) => end,
            None => {
                out.push(Piece::Lit("&#60;"));
                i + 1
            }
        };
    }
    let Some((tag, end)) =
        read_tag(body, i, Some(dead)).filter(|(tag, _)| !tag_head(tag).name.is_empty())
    else {
        out.push(Piece::Lit("&#60;"));
        return i + 1;
    };
    let sanitized = sanitize_tag(tag, ctx, tokens, inked);
    push_emitted(out, tokens.len(), sanitized.emit);
    match sanitized.drop {
        Some(name) => skip_drop_content(body, end, name, dead),
        None => end,
    }
}

/// Reads a bare address at body[i], answering the index after it, or None when none starts there.
pub fn bare_at(
    body: &str,
    i: usize,
    ctx: &Ctx,
    style_links: bool,
    tokens: &mut Vec<String>,
    out: &mut Vec<Piece>,
) -> Option<usize> {
    let (bare_url, bare_href, bare_end) = read_barelink(body, i)?;
    if !target_allowed(&bare_href) {
        return None;
    }
    let mut url = bare_url.as_str();
    let mut href = bare_href.as_str();
    // GFM leaves a trailing entity reference such as "&hl;" out of the address.
    if at(body, bare_end) == b';' {
        let alnum = url
            .bytes()
            .rev()
            .take_while(u8::is_ascii_alphanumeric)
            .count();
        if alnum > 0 && url.len() > alnum && at(url, url.len() - alnum - 1) == b'&' {
            let cut = alnum + 1;
            url = &url[..url.len() - cut];
            href = &href[..href.len() - cut];
        }
    }
    let html = if style_links {
        link_html(url, href, ctx.ink, false)
    } else {
        None
    };
    match html {
        Some(html) => {
            tokens.push(html);
            out.push(Piece::Token(tokens.len() - 1));
        }
        None => out.push(Piece::Text(escape_html_text(url))),
    }
    Some(bare_end - (bare_url.len() - url.len()))
}
