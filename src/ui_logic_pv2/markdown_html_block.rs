//! Images leave an HTML block as image blocks, top-level HTML blocks start their own lines, and HTML headings are read.

use super::markdown_html_image::skip_space;
use super::markdown_text::{at, blank, char_at, is_space, is_word};
use std::collections::HashMap;

#[path = "markdown_html_split.rs"]
mod split;
pub use split::{split_html_images, Part, ROW_IMAGE_CAP};

// The names CommonMark starts an HTML block with, besides h1 to h6.
const BLOCK_NAMES: &[&str] = &[
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "meta",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "section",
    "source",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];
const BLOCK_TAG_NAMES: &[&str] = &["p", "div", "table", "ul", "ol", "blockquote", "pre"];
const BOUNDARY_NAMES: &[&str] = &[
    "p",
    "div",
    "table",
    "tr",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
];
const LINE_OPENER_NAMES: &[&str] = &[
    "p",
    "div",
    "details",
    "summary",
    "hr",
    "table",
    "ul",
    "ol",
    "blockquote",
    "pre",
];
const NESTING_NAMES: &[&str] = &[
    "p",
    "div",
    "details",
    "summary",
    "table",
    "ul",
    "ol",
    "blockquote",
    "pre",
];

pub fn is_heading_name(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() == 2 && b[0] == b'h' && (b'1'..=b'6').contains(&b[1])
}

fn named(name: &str, names: &[&str], headings: bool) -> bool {
    names.contains(&name) || (headings && is_heading_name(name))
}

// The lowercased run of ASCII letters and digits at i.
fn name_at(text: &str, i: usize) -> String {
    let len = text.as_bytes()[i.min(text.len())..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    text[i..i + len].to_ascii_lowercase()
}

fn spaces_up_to_three(line: &str) -> usize {
    line.bytes().take(3).take_while(|b| *b == b' ').count()
}

/// True when a line starts an HTML block: '<div align="center">' and '  </table>' do, '<span>x</span>' and 'text <div>' do not.
pub fn block_start(line: &str) -> bool {
    let i = spaces_up_to_three(line);
    if at(line, i) != b'<' {
        return false;
    }
    let j = i + 1 + usize::from(at(line, i + 1) == b'/');
    let name = name_at(line, j);
    if !named(&name, BLOCK_NAMES, true) {
        return false;
    }
    let k = j + name.len();
    match char_at(line, k) {
        None | Some('>') => true,
        Some('/') => at(line, k + 1) == b'>',
        Some(c) => is_space(c),
    }
}

/// True for one complete tag alone on its line: '<a href="x">' and '</span>' are, '<a href="x">text' is not.
pub fn complete_tag(line: &str) -> bool {
    let i = spaces_up_to_three(line);
    if at(line, i) != b'<' {
        return false;
    }
    let closing = at(line, i + 1) == b'/';
    let j = i + 1 + usize::from(closing);
    if !at(line, j).is_ascii_alphabetic() {
        return false;
    }
    let k = j + line.as_bytes()[j..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || **b == b'-')
        .count();
    let gt = if closing {
        let gt = skip_space(line, k);
        if at(line, gt) != b'>' {
            return false;
        }
        gt
    } else {
        match at(line, k) {
            b'>' => k,
            b'/' if at(line, k + 1) == b'>' => k + 1,
            _ if char_at(line, k).is_some_and(is_space) => match line[k..].find(['<', '>']) {
                Some(stop) if at(line, k + stop) == b'>' => k + stop,
                _ => return false,
            },
            _ => return false,
        }
    };
    blank(&line[gt + 1..])
}

// The index after a tag `<name\b` at i whose name is one of names, any case, closer allowed; the word boundary is ASCII.
fn tag_word_at(text: &str, i: usize, names: &[&str], headings: bool, slash: bool) -> bool {
    if at(text, i) != b'<' {
        return false;
    }
    let j = i + 1 + usize::from(slash && at(text, i + 1) == b'/');
    let len = text.as_bytes()[j.min(text.len())..]
        .iter()
        .take_while(|b| is_word(**b))
        .count();
    named(&text[j..j + len].to_ascii_lowercase(), names, headings)
}

fn holds_tag_word(text: &str, names: &[&str], headings: bool, slash: bool) -> bool {
    text.match_indices('<')
        .any(|(i, _)| tag_word_at(text, i, names, headings, slash))
}

/// True for a line holding '<br' with a closing ">" before any "<".
fn holds_break(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.match_indices("<br").any(|(i, _)| {
        !is_word(at(&lower, i + 3))
            && lower[i + 3..]
                .find(['<', '>'])
                .is_some_and(|stop| at(&lower, i + 3 + stop) == b'>')
    })
}

/// Calls step(closing) for each nesting block tag in the line, in order: '<div class="x">' steps open, '</div>' closed.
pub fn nesting_tags(line: &str, mut step: impl FnMut(bool)) {
    let mut i = 0;
    while let Some(lt) = line[i..].find('<').map(|lt| lt + i) {
        let closing = at(line, lt + 1) == b'/';
        let from = lt + 1 + usize::from(closing);
        let name = name_at(line, from);
        let after = at(line, from + name.len());
        if named(&name, NESTING_NAMES, true)
            && (matches!(after, b'>' | b'/')
                || char_at(line, from + name.len()).is_some_and(is_space))
        {
            if let Some(stop) = line[from..].find(['<', '>']).map(|stop| stop + from) {
                if at(line, stop) == b'>' {
                    step(closing);
                    i = stop + 1;
                    continue;
                }
            }
        }
        i = lt + 1;
    }
}

/// The open block HTML a run holds after a line: ('<div>', 0) answers 1, ('</div>', 1) answers 0.
pub fn nest_depth(line: &str, mut depth: usize) -> usize {
    nesting_tags(line, |closing| {
        depth = if closing {
            depth.saturating_sub(1)
        } else {
            depth + 1
        }
    });
    depth
}

/// Calls each(start, end, slash, name) for every tag `<\/?[A-Za-z][A-Za-z0-9]*[^<>]*>` in the text, in order.
fn any_tags(text: &str, mut each: impl FnMut(usize, usize, bool, &str)) {
    let mut i = 0;
    while let Some(lt) = text[i..].find('<').map(|lt| lt + i) {
        let slash = at(text, lt + 1) == b'/';
        let from = lt + 1 + usize::from(slash);
        if at(text, from).is_ascii_alphabetic() {
            let name = name_at(text, from);
            if let Some(stop) = text[from..].find(['<', '>']).map(|stop| stop + from) {
                if at(text, stop) == b'>' {
                    each(lt, stop + 1, slash, &name);
                    i = stop + 1;
                    continue;
                }
            }
        }
        i = lt + 1;
    }
}

fn without_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut plain = 0;
    any_tags(text, |start, end, _, _| {
        out.push_str(&text[plain..start]);
        plain = end;
    });
    out.push_str(&text[plain..]);
    out
}

/// The text with each closer that no earlier opener matches dropped: '<td>a</p>' answers '<td>a'.
pub fn without_stray_closers(text: &str) -> String {
    let mut open: HashMap<String, usize> = HashMap::new();
    let mut out = String::with_capacity(text.len());
    let mut plain = 0;
    any_tags(text, |start, end, slash, name| {
        if !slash {
            *open.entry(name.to_string()).or_default() += 1;
            return;
        }
        match open.get_mut(name) {
            Some(count) if *count > 0 => *count -= 1,
            _ => {
                out.push_str(&text[plain..start]);
                plain = end;
            }
        }
    });
    out.push_str(&text[plain..]);
    out
}

/// Each top-level HTML block starts after a blank line: ['<h1>A</h1>', '<p>B</p>'] answers ['<h1>A</h1>', '', '<p>B</p>'].
pub fn separate_blocks(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut depth = 0;
    for line in lines {
        if blank(line) {
            depth = 0;
        } else if depth == 0 && out.last().is_some_and(|last| !blank(last)) && line_opener(line) {
            out.push(String::new());
        }
        out.push(line.clone());
        depth = nest_depth(line, depth);
    }
    out
}

// '<h1 align="center">Flea</h1>' and '  <details open>' start a line of their own; '<b>x</b>' does not.
fn line_opener(line: &str) -> bool {
    let i = spaces_up_to_three(line);
    if at(line, i) != b'<' {
        return false;
    }
    let name = name_at(line, i + 1);
    let after = i + 1 + name.len();
    named(&name, LINE_OPENER_NAMES, true)
        && (at(line, after) == b'>'
            || at(line, after) == b'/'
            || char_at(line, after).is_some_and(is_space))
}
