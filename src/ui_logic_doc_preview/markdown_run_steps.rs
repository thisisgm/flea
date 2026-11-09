//! The scan's own steps: citations numbered, pieces rendered, code and math spans held, and a closing bracket made a link or image.

use super::super::markdown_emph::{literal, process};
use super::super::markdown_inline::{Interval, SpanKind};
use super::super::markdown_piece::{LineState, Piece};
use super::super::markdown_resolve::{read_destination, resolve_pair, styled_span, PairHost};
use super::super::Ctx;
use super::{count_frame_step, LastSpan, Scan};
use std::collections::HashMap;

impl Scan<'_, '_> {
    // Number a citation the first time it is read, so its superscript is the next free number.
    fn cite(&mut self, token: usize) {
        let id = self.citations[&token].clone();
        if let Some(cited) = self.env.cited.as_deref_mut() {
            if self.env.numbers.get(&id) == Some(&0) {
                cited.push(id.clone());
                self.env.numbers.insert(id.clone(), cited.len());
            }
        }
        let number = self.env.numbers.get(&id).copied().unwrap_or(0);
        self.env.tokens[token] = format!("<sup>{number}</sup>");
    }

    // Number the citations before out[limit], which a label rendered now would otherwise number ahead of.
    fn cite_before(&mut self, limit: usize) {
        let to = limit.min(self.image_mark.unwrap_or(limit));
        while self.numbered < to {
            if let Piece::Token(token) = self.out[self.numbered] {
                if self.citations.contains_key(&token) {
                    self.cite(token);
                }
            }
            self.numbered += 1;
        }
    }

    // Join text, tokens and emphasis marks; inside emphasis or a label a newline is a space.
    pub(super) fn render_from(&mut self, from: usize, flat: bool, alt: bool) -> String {
        let mut parts = String::new();
        let mut depth: isize = if flat { 1 } else { 0 };
        for k in from..self.out.len() {
            match &self.out[k] {
                Piece::Delim(d) => {
                    let delim = &self.delims.list[*d];
                    depth -= delim.closes as isize;
                    parts.push_str(&delim.before);
                    parts.push_str(&literal(delim));
                    parts.push_str(&delim.after);
                    depth += delim.opens as isize;
                }
                Piece::Token(token) => {
                    let token = *token;
                    if let Some(id) = self.citations.get(&token) {
                        if alt {
                            parts.push_str(&format!("[^{id}]"));
                            continue;
                        }
                        self.cite(token);
                    }
                    parts.push_str(&self.env.tokens[token]);
                }
                piece => {
                    if depth > 0 && piece.is("\n") {
                        parts.push(' ');
                    } else if let Some(text) = piece.text() {
                        parts.push_str(&text);
                    }
                }
            }
        }
        parts
    }

    // The label of the bracket pair opened at out[mark], its emphasis resolved first.
    fn label_html(&mut self, mark: usize, alt: bool) -> String {
        self.cite_before(mark);
        let mut from = self.delims.list.len();
        while from > 0 && self.delims.list[from - 1].at > mark {
            from -= 1;
        }
        process(&mut self.delims.list, from);
        self.render_from(mark + 1, true, alt)
    }

    // The open brackets of a finished paragraph never pair with a later "]", so they stay literal.
    pub(super) fn end_paragraph(&mut self) {
        self.open.clear();
        self.active_links.clear();
        self.image_mark = None;
    }
}

struct Label<'x, 's, 'e> {
    scan: &'x mut Scan<'s, 'e>,
    mark: usize,
    bang: bool,
}

impl PairHost for Label<'_, '_, '_> {
    fn label(&mut self) -> Option<String> {
        Some(self.scan.label_html(self.mark, self.bang))
    }
    fn tokens(&mut self) -> &mut Vec<String> {
        self.scan.env.tokens
    }
}

// Plain prose returns directly; a plain table cell takes the bulk escaper before any markup is emitted.
pub(super) fn plain_fast(body: &str, literal_plain: bool) -> bool {
    let bytes = body.as_bytes();
    if bytes.iter().any(|b| b"`$[]<>\\!*_".contains(b))
        || body.contains("  \n")
        || body.contains("http://")
        || body.contains("https://")
        || body.contains("www.")
    {
        return false;
    }
    if !literal_plain {
        return !bytes
            .windows(2)
            .any(|w| w[0] == b'&' && (w[1] == b'#' || w[1].is_ascii_alphabetic()));
    }
    !(bytes.contains(&b'~')
        || (0..bytes.len()).any(|i| bytes[i] == b'&' && reference_shape(body, i)))
}

// "&#12;", "&#x1F;" or "&name;" at i, of any length, as the plain fast path refuses.
fn reference_shape(body: &str, i: usize) -> bool {
    let rest = &body.as_bytes()[i + 1..];
    let (digits, start) = match rest.first() {
        Some(b'#') => match rest.get(1) {
            Some(b'x' | b'X') => (
                rest[2..]
                    .iter()
                    .take_while(|b| b.is_ascii_hexdigit())
                    .count(),
                2,
            ),
            _ => (
                rest[1..].iter().take_while(|b| b.is_ascii_digit()).count(),
                1,
            ),
        },
        Some(b) if b.is_ascii_alphabetic() => (
            rest.iter()
                .take_while(|b| b.is_ascii_alphanumeric())
                .count(),
            0,
        ),
        _ => return false,
    };
    digits > 0 && rest.get(start + digits) == Some(&b';')
}

// A code or math span as one held token; a span that repeats the one before it reuses that token.
pub(super) fn hold(
    body: &str,
    span: Interval,
    chrome: Option<&str>,
    cache: &mut HashMap<(SpanKind, String), String>,
    last: &mut LastSpan,
    scan: &mut Scan,
) {
    let Some(chrome) = chrome else {
        scan.env.tokens.push(body[span.from..span.to].to_string());
        scan.out.push(Piece::Token(scan.env.tokens.len() - 1));
        return;
    };
    let inner_start = span.from + span.len;
    let inner_end = span.to - span.len;
    let inner = &body[inner_start..inner_end];
    let same = last.start.is_some_and(|start| {
        span.kind == last.kind
            && span.len == last.open
            && inner.len() == last.length
            && body[start..start + last.length] == *inner
    });
    if same {
        scan.out.push(Piece::Token(last.token));
        return;
    }
    let mut text = inner.replace('\n', " ");
    if text.len() >= 2 && text.starts_with(' ') && text.ends_with(' ') {
        text = text[1..text.len() - 1].to_string();
    } else if text == " " {
        text.clear();
    }
    scan.env
        .tokens
        .push(styled_span(span.kind, &text, chrome, cache));
    *last = LastSpan {
        start: Some(inner_start),
        length: inner.len(),
        open: span.len,
        kind: span.kind,
        token: scan.env.tokens.len() - 1,
    };
    scan.out.push(Piece::Token(last.token));
}

// A "]" with a bracket open: a link or image when a destination follows, else literal; answers the index after it.
pub(super) fn close_bracket(
    body: &str,
    i: usize,
    ctx: &Ctx,
    scan: &mut Scan,
    line: &mut LineState,
) -> usize {
    let index = scan.open.pop().unwrap_or(0);
    count_frame_step();
    if Some(scan.open.len()) == scan.image_depth {
        scan.image_mark = None;
    }
    let (bang, mark, raw_start, active, passthrough) = {
        let frame = &scan.frames[index];
        (
            frame.bang,
            frame.mark,
            frame.raw_start,
            frame.active,
            frame.passthrough,
        )
    };
    if !bang && active {
        scan.active_links.pop();
    }
    if passthrough {
        scan.out.push(Piece::Lit("&#93;"));
        return i + 1;
    }
    let raw = &body[raw_start..i];
    let mut j = i + 1;
    let mut made = None;
    if active {
        if let Some((url, end)) = read_destination(body, j, raw, scan.env.defs) {
            let mut host = Label { scan, mark, bang };
            made = Some(resolve_pair(raw, &url, bang, ctx, &mut host));
            j = end;
        }
        if made.is_some() && !bang {
            while let Some(link) = scan.active_links.pop() {
                count_frame_step();
                scan.frames[link].active = false;
            }
        }
    }
    let Some(made) = made else {
        scan.out.push(Piece::Lit("&#93;"));
        return i + 1;
    };
    let keep = if bang { mark - 1 } else { mark };
    scan.out.truncate(keep);
    scan.numbered = scan.numbered.min(keep);
    while scan.delims.list.last().is_some_and(|d| d.at >= keep) {
        scan.delims.list.pop();
    }
    scan.out.push(made);
    line.restart(scan.out.len(), scan.out.last().unwrap_or(&Piece::Lit("")));
    j
}
