//! One run's lines cut into paragraphs: a blank line ends the paragraph unless HTML spans it.

use super::markdown_html::{read_tag, tag_head, MAX_TAG_LENGTH};
use super::markdown_text::{at, blank, find_byte, find_str};
use std::collections::HashMap;

// Tags that never pair across a blank line: void elements carry no content, so an unclosed one never joins paragraphs.
fn void_tag(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "hr"
            | "img"
            | "link"
            | "meta"
            | "source"
            | "track"
            | "wbr"
    )
}

/// The code spans of a text as CommonMark reads them: a backtick run closed by a run of the same length; an unclosed run stays literal.
pub fn code_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    let mut open_by_len: HashMap<usize, usize> = HashMap::new();
    let mut openers: Vec<usize> = Vec::new();
    let mut next = find_byte(text, b'`', 0);
    while let Some(i) = next {
        let mut j = i + 1;
        while at(text, j) == b'`' {
            j += 1;
        }
        let len = j - i;
        if let Some(open) = open_by_len.get(&len).copied() {
            while out.last().is_some_and(|last| last.0 > open) {
                out.pop();
            }
            out.push((open, j));
            while let Some(covered) = openers.pop() {
                open_by_len.remove(&covered);
                if covered == len {
                    break;
                }
            }
        } else {
            open_by_len.insert(len, i);
            openers.push(len);
        }
        next = find_byte(text, b'`', j);
    }
    out
}

/// True when an odd run of backslashes stands before text[tag], so "\<" opens no tag.
pub fn is_escaped(text: &str, tag: usize) -> bool {
    text.as_bytes()[..tag]
        .iter()
        .rev()
        .take_while(|b| **b == b'\\')
        .count()
        % 2
        == 1
}

/// After which blank lines the run's paragraphs end: ["a", "", "b"] cuts after the blank, ["<div>", "", "x"] never inside the open tag.
pub fn cut_after<S: AsRef<str>>(lines: &[S]) -> Vec<bool> {
    let mut cut = vec![false; lines.len()];
    let mut later_text = vec![false; lines.len()];
    let mut seen = false;
    for w in (0..lines.len()).rev() {
        later_text[w] = seen;
        if !blank(lines[w].as_ref()) {
            seen = true;
        }
    }
    let mut depth = 0usize;
    let mut comment = false;
    // An unterminated tag may complete on a later line; past the tag reader's own cap it never classifies, so it is dropped.
    let mut carry = String::new();
    let mut has_text = false;
    for (l, raw) in lines.iter().enumerate() {
        let line = raw.as_ref();
        if blank(line) {
            if has_text && later_text[l] && depth == 0 && !comment {
                cut[l] = true;
                has_text = false;
                carry.clear();
            }
            continue;
        }
        has_text = true;
        // A tag may wrap at whitespace, so the carried fragment rejoins with the newline it was split on.
        let text = if carry.is_empty() {
            line.to_string()
        } else {
            format!("{carry}\n{line}")
        };
        carry.clear();
        // Tags inside a closed code span are literal text, the same rule the inline parser pairs by.
        let code = code_ranges(&text);
        let mut span = 0;
        let mut i = 0;
        // A ">" search that reaches the end proves no later tag on this text can close, so each pays once.
        let mut no_close: Option<usize> = None;
        while i < text.len() {
            if comment {
                let Some(end) = find_str(&text, "-->", i) else {
                    break;
                };
                comment = false;
                i = end + 3;
                continue;
            }
            let Some(tag) = find_byte(&text, b'<', i) else {
                break;
            };
            while span < code.len() && code[span].1 <= tag {
                span += 1;
            }
            if span < code.len() && code[span].0 <= tag && tag < code[span].1 {
                i = code[span].1;
                continue;
            }
            if is_escaped(&text, tag) {
                i = tag + 1;
                continue;
            }
            if text[tag..].starts_with("<!--") {
                comment = true;
                i = tag + 4;
                continue;
            }
            let found = if no_close.is_none_or(|stop| tag <= stop) {
                read_tag(&text, tag, None)
            } else {
                None
            };
            let Some((found, end)) = found else {
                if no_close.is_none() && find_byte(&text, b'>', tag + 1).is_none() {
                    no_close = Some(tag);
                }
                i = tag + 1;
                continue;
            };
            let head = tag_head(found);
            if !head.name.is_empty() && !void_tag(&head.name) {
                if head.closing {
                    depth = depth.saturating_sub(1);
                } else if !head.self_close {
                    depth += 1;
                }
            }
            i = end;
        }
        let last_lt = text.rfind('<');
        let last_gt = text.rfind('>');
        if let Some(lt) = last_lt {
            if last_gt.is_none_or(|gt| lt > gt) && text.len() - lt <= MAX_TAG_LENGTH {
                carry = text[lt..].to_string();
            }
        }
    }
    cut
}
