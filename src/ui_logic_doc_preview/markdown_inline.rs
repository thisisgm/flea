//! Single-pass scanners that never restart: code and math spans, and the markup a held span or a link becomes.

use super::markdown_escape::{escape_text, push_escaped, push_reference, CHIP_PAD};
use super::markdown_text::{at, find_byte, is_colour};

const MAX_MATH_RUN_LENGTH: usize = 2;

/// What a held interval holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpanKind {
    Code,
    Math,
}

/// One held interval: its start, its end past the closer, its delimiter run length and its kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    pub from: usize,
    pub to: usize,
    pub len: usize,
    pub kind: SpanKind,
}

/// The blanks the inline readers skip: space, tab, newline and carriage return.
pub fn is_blank_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// True for ASCII punctuation.
pub fn is_punct(b: u8) -> bool {
    super::markdown_escape::is_ascii_punct(b as u32)
}

/// The code and math spans of a text in order: code pairs before math, never across it.
pub fn span_intervals(text: &str) -> Vec<Interval> {
    let has_code = text.contains('`');
    if !has_code && !text.contains('$') {
        return Vec::new();
    }
    let code = if has_code {
        code_intervals(text)
    } else {
        Vec::new()
    };
    if text.contains('$') {
        let maths = math_intervals(text, &code);
        merge(code, maths)
    } else {
        code
    }
}

// "``a ` b``" and "`c`": a closing run drops the openers and spans inside it, which are literal.
fn code_intervals(text: &str) -> Vec<Interval> {
    let mut out: Vec<Interval> = Vec::new();
    let mut open_by_len: std::collections::HashMap<usize, usize> = Default::default();
    let mut openers: Vec<usize> = Vec::new();
    let mut i = find_byte(text, b'`', 0);
    while let Some(start) = i {
        let mut j = start + 1;
        while at(text, j) == b'`' {
            j += 1;
        }
        let len = j - start;
        if let Some(&open) = open_by_len.get(&len) {
            while out.last().is_some_and(|last| last.from > open) {
                out.pop();
            }
            out.push(Interval {
                from: open,
                to: j,
                len,
                kind: SpanKind::Code,
            });
            while let Some(covered) = openers.pop() {
                open_by_len.remove(&covered);
                if covered == len {
                    break;
                }
            }
        } else {
            open_by_len.insert(len, start);
            openers.push(len);
        }
        i = find_byte(text, b'`', j);
    }
    out
}

// "costs $5 and $10" stays prose; "$x+1$" pairs; no space inside an edge, no digit after the closer, no odd backslash run before.
fn math_intervals(text: &str, code: &[Interval]) -> Vec<Interval> {
    let mut out = Vec::new();
    let mut code_at = 0;
    let mut i = find_byte(text, b'$', 0);
    let mut open: Option<usize> = None;
    let mut open_len = 0;
    while let Some(start) = i {
        while code_at < code.len() && code[code_at].to <= start {
            if open.is_some_and(|o| code[code_at].from > o) {
                open = None;
            }
            code_at += 1;
        }
        if code_at < code.len() && code[code_at].from < start {
            open = None;
            i = find_byte(text, b'$', code[code_at].to);
            continue;
        }
        let mut j = start + 1;
        while at(text, j) == b'$' {
            j += 1;
        }
        let len = j - start;
        let mut back = 0;
        let mut b = start;
        while b > 0 && at(text, b - 1) == b'\\' {
            back += 1;
            b -= 1;
        }
        if len <= MAX_MATH_RUN_LENGTH && back % 2 == 0 {
            let after = at(text, j);
            let can_close =
                start > 0 && !is_blank_byte(at(text, start - 1)) && !after.is_ascii_digit();
            if open.is_some() && len == open_len && can_close {
                out.push(Interval {
                    from: open.unwrap_or(0),
                    to: j,
                    len: open_len,
                    kind: SpanKind::Math,
                });
                open = None;
            } else if j < text.len() && !is_blank_byte(at(text, j)) {
                open = Some(start);
                open_len = len;
            }
        }
        i = find_byte(text, b'$', j);
    }
    out
}

// Two from-ordered lists merged in one pass; code and math never share a start.
fn merge(a: Vec<Interval>, b: Vec<Interval>) -> Vec<Interval> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut ai, mut bi) = (0, 0);
    while ai < a.len() || bi < b.len() {
        if bi >= b.len() || (ai < a.len() && a[ai].from < b[bi].from) {
            out.push(a[ai]);
            ai += 1;
        } else {
            out.push(b[bi]);
            bi += 1;
        }
    }
    out
}

/// An inline code span as a chip, its content escaped; math keeps its kind for the figure pass. None without a usable chrome.
pub fn code_html(content: &str, chrome: &str, kind: SpanKind) -> Option<String> {
    if !is_colour(chrome) {
        return None;
    }
    let mut out = String::with_capacity(content.len() + 128);
    out.push_str(if kind == SpanKind::Math {
        "<code data-math=\"inline\" style=\"background-color:"
    } else {
        "<code style=\"background-color:"
    });
    out.push_str(chrome);
    out.push_str("\">");
    out.push_str(CHIP_PAD);
    push_escaped(&mut out, content);
    out.push_str(CHIP_PAD);
    out.push_str("</code>");
    Some(out)
}

/// Text with its ASCII punctuation as numeric references.
pub fn escape_html_text(content: &str) -> String {
    escape_text(content)
}

/// Text a reference decoded to: punctuation and digits as numeric references, so no marker, fence or rule forms at a line start.
pub fn escape_decoded_text(content: &str) -> String {
    let mut out = String::with_capacity(content.len() * 2);
    for c in content.chars() {
        if c.is_ascii_digit() || super::markdown_escape::is_ascii_punct(c as u32) {
            push_reference(&mut out, c as u32);
        } else {
            out.push(c);
        }
    }
    out
}

/// The tags a link wears around its label, or None when the ink is no colour.
pub fn link_ink(ink: &str) -> Option<(String, &'static str)> {
    is_colour(ink).then(|| (format!("<font color=\"{ink}\">"), "</font>"))
}

/// A link as an anchor wrapping its label in the ink; markup says the label is already markup.
pub fn link_html(label: &str, url: &str, ink: &str, markup: bool) -> Option<String> {
    let (open, close) = link_ink(ink)?;
    let safe = url.replace('&', "&#38;").replace('"', "&#34;");
    let label = if markup {
        label.to_string()
    } else {
        escape_html_text(label)
    };
    Some(format!("<a href=\"{safe}\">{open}{label}{close}</a>"))
}
