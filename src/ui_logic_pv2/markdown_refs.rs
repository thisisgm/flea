//! Link reference and footnote definitions: their readers, the maps they fill and the lines they hide.

use super::markdown_container::{
    indentation_at, quote_at, starts_block, take_indent, take_quote, text_at, View, CODE_INDENT,
};
use super::markdown_html::{read_tag, tag_head, Dead};
use super::markdown_link::{angle_close, normalize_label, read_label_ref};
use super::markdown_text::{at, blank, find_byte, is_space, trim};
use std::collections::{HashMap, HashSet};

const MAX_INDENT: usize = 3;

/// The definitions a document collects, shared by every pass and every nested writer.
#[derive(Clone, Debug, Default)]
pub struct Refs {
    /// Link labels, normalized, to their destinations; the first definition of a label wins.
    pub defs: HashMap<String, String>,
    /// Footnote ids to their text.
    pub notes: HashMap<String, String>,
    /// Footnote ids to the number their first citation gave them, 0 before it.
    pub numbers: HashMap<String, usize>,
}

/// How a hidden line replays: as nothing, or as the paragraph state its definition opener left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hidden {
    Line,
    Paragraph,
}

/// What one pass learns about its lines: which hide, which are code, which keep a definition's bracket literal.
#[derive(Clone, Debug, Default)]
pub struct Lines {
    pub hidden: HashMap<usize, Hidden>,
    pub escaped: HashSet<usize>,
    pub code: HashSet<usize>,
    pub dropped: Vec<(usize, usize)>,
    pub deep: bool,
}

/// A definition's body: its destination and whether a title closed its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body {
    pub target: String,
    pub titled: bool,
    pub end: usize,
}

/// A one-line definition: its key, destination ("" when it continues on the next line) and title flag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    pub key: String,
    pub target: String,
    pub titled: bool,
}

/// One definition's line, "[pic]: image.png \"Title\""; an empty destination can continue on the next container line.
pub fn read_definition(line: &str) -> Option<Definition> {
    let mut indent = 0;
    while indent < MAX_INDENT && at(line, indent) == b' ' {
        indent += 1;
    }
    let (label, end) = read_label_ref(line, indent)?;
    if blank(label) || label.starts_with('^') || at(line, end) != b':' {
        return None;
    }
    let rest = &line[end + 1..];
    if blank(rest) {
        return Some(Definition {
            key: normalize_label(label),
            target: String::new(),
            titled: false,
        });
    }
    let body = read_body_at(rest, 0)?;
    Some(Definition {
        key: normalize_label(label),
        target: body.target,
        titled: body.titled,
    })
}

/// A destination line on its own, "<my pic.png> \"Title\"" or "pic.png": the target ("" when none) and title flag.
pub fn read_definition_parts(text: &str) -> (String, bool) {
    read_body_at(text, 0).map_or((String::new(), false), |body| (body.target, body.titled))
}

// "\n  x" read from 0 answers (4, true); spaces, tabs and at most one line ending are skipped.
fn skip_gap(text: &str, from: usize) -> (usize, bool) {
    let mut i = from;
    let mut newline = false;
    while i < text.len()
        && (matches!(at(text, i), b' ' | b'\t') || (!newline && at(text, i) == b'\n'))
    {
        newline = newline || at(text, i) == b'\n';
        i += 1;
    }
    (i, newline)
}

fn space_or_control(c: char) -> bool {
    is_space(c) || (c as u32) < 0x20
}

// '<a b> "t"' from 0 answers ("a b", 5); "/u x" answers ("/u", 2); "<a" answers None.
fn read_destination_at(text: &str, from: usize) -> Option<(String, usize)> {
    if at(text, from) == b'<' {
        let close = angle_close(text, from + 1, text.len(), true)?;
        return Some((text[from + 1..close].to_string(), close + 1));
    }
    let mut i = from;
    let mut depth: i64 = 0;
    while let Some(c) = super::markdown_text::char_at(text, i) {
        if space_or_control(c) {
            break;
        }
        depth += match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        let next = super::markdown_text::char_at(text, i + 1);
        i += if c == '\\' && next.is_some_and(|n| !is_space(n)) {
            1 + next.map_or(0, char::len_utf8)
        } else {
            c.len_utf8()
        };
    }
    (i > from && depth == 0).then(|| (text[from..i].to_string(), i))
}

// '"a\nb" x' from 0 answers 6, past the closing quote; a title needs its closer, and a "(" title holds no "(".
fn read_title_end(text: &str, from: usize) -> Option<usize> {
    let opener = at(text, from);
    let closer = match opener {
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    let mut i = from + 1;
    while i < text.len() {
        let c = at(text, i);
        if c == closer {
            return Some(i + 1);
        }
        if opener == b'(' && c == b'(' {
            return None;
        }
        if c == b'\\' && at(text, i + 1) != b'\n' {
            i += 1;
        }
        i += 1;
    }
    None
}

// True when only blanks remain on the line from from.
fn ends_line(text: &str, from: usize) -> bool {
    let mut i = from;
    while matches!(at(text, i), b' ' | b'\t') {
        i += 1;
    }
    i >= text.len() || at(text, i) == b'\n'
}

/// A definition's body from from: '<a b> "t"' answers ("a b", titled, 9); "/u x" answers None, junk follows the destination.
pub fn read_body_at(text: &str, from: usize) -> Option<Body> {
    let (lead, _) = skip_gap(text, from);
    let (target, end) = read_destination_at(text, lead)?;
    let (gap, _) = skip_gap(text, end);
    let close = if gap > end {
        read_title_end(text, gap)
    } else {
        None
    };
    let titled = close.is_some_and(|close| ends_line(text, close));
    if !titled && !ends_line(text, end) {
        return None;
    }
    Some(Body {
        target,
        titled,
        end: if titled { close.unwrap_or(end) } else { end },
    })
}

/// A definition that may span lines from lines[from]: its key, target, title flag and last line.
pub fn definition_at(lines: &[&str], from: usize) -> Option<(String, String, bool, usize)> {
    let mut stop = from + 1;
    while stop < lines.len() && !blank(lines[stop]) && !starts_block(lines[stop]) {
        stop += 1;
    }
    let text = lines[from..stop].join("\n");
    let mut indent = 0;
    while indent < MAX_INDENT && at(&text, indent) == b' ' {
        indent += 1;
    }
    let (label, end) = read_label_ref(&text, indent)?;
    if blank(label) || label.starts_with('^') || at(&text, end) != b':' {
        return None;
    }
    let body = read_body_at(&text, end + 1)?;
    let last = from + text[..body.end].bytes().filter(|b| *b == b'\n').count();
    Some((normalize_label(label), body.target, body.titled, last))
}

/// A container frame as a definition's title replay needs it: a quote, or a list item's content column.
#[derive(Clone, Copy, Debug)]
pub enum Prefix {
    Quote,
    Item(usize),
}

// '    > "t"' under an item (content column 4) then a quote answers '"t"'; a line opening its own quote answers None.
fn replayed(line: &str, frames: &[Prefix]) -> Option<String> {
    let mut view = View::default();
    for frame in frames {
        match frame {
            Prefix::Quote => match quote_at(line, &view) {
                Some(quote) => take_quote(line, &mut view, quote),
                None => break,
            },
            Prefix::Item(col) => {
                if indentation_at(line, &view, *col).1 >= *col {
                    take_indent(line, &mut view, *col);
                } else {
                    break;
                }
            }
        }
    }
    quote_at(line, &view)
        .is_none()
        .then(|| text_at(line, &view))
}

// "'a" over "b'" at 0 with no frames answers 1, the line that closes the title; no closer, or junk after it, answers None.
fn title_end(lines: &[&str], from: usize, frames: &[Prefix]) -> Option<usize> {
    let first = replayed(lines.get(from)?, frames)?;
    let open = trim(&first).bytes().next()?;
    if !matches!(open, b'"' | b'\'' | b'(') {
        return None;
    }
    let close = if open == b'(' { b')' } else { open };
    for (k, line) in lines.iter().enumerate().skip(from) {
        let text = replayed(line, frames)?;
        if blank(&text) {
            return None;
        }
        let mut c = if k == from {
            find_byte(&text, open, 0).map_or(0, |at| at + 1)
        } else {
            0
        };
        while c < text.len() {
            if at(&text, c) == b'\\' {
                c += 1;
            } else if at(&text, c) == close {
                return blank(&text[c + 1..]).then_some(k);
            }
            c += 1;
        }
    }
    None
}

/// Stores a definition unless its label already has one.
pub fn store_definition(defs: &mut HashMap<String, String>, key: String, target: String) {
    defs.entry(key).or_insert(target);
}

/// Hides the title lines after a finished definition; allowed says it has none yet, frames are its containers.
pub fn hide_title(
    lines: &[&str],
    from: usize,
    state: &mut Lines,
    allowed: bool,
    frames: &[Prefix],
) {
    let Some(end) = allowed.then(|| title_end(lines, from, frames)).flatten() else {
        return;
    };
    for k in from..=end {
        state.hidden.insert(k, Hidden::Line);
    }
    state.dropped.push((from, end));
}

/// A footnote definition line, "[^id]: note body": its id and text.
pub fn read_footnote_definition(line: &str) -> Option<(String, String)> {
    let mut i = 0;
    while i < MAX_INDENT && at(line, i) == b' ' {
        i += 1;
    }
    if !line[i..].starts_with("[^") {
        return None;
    }
    let from = i + 2;
    let close = line[from..].find([']', '\n']).map(|c| c + from)?;
    if close == from || at(line, close) != b']' || at(line, close + 1) != b':' {
        return None;
    }
    let rest = line[close + 2..].trim_start_matches(is_space);
    if rest.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    Some((line[from..close].to_string(), rest.to_string()))
}

/// A definition-shaped line the block reader refused, its bracket escaped so the renderer cannot read it as one.
pub fn kill_definition(line: &str) -> String {
    let mut i = 0;
    while i < MAX_INDENT && at(line, i) == b' ' {
        i += 1;
    }
    if at(line, i) != b'[' {
        return line.to_string();
    }
    let mut j = i + 1;
    let mut any = false;
    loop {
        match at(line, j) {
            b'\\'
                if super::markdown_text::char_at(line, j + 1)
                    .is_some_and(|c| !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')) =>
            {
                j += 1 + super::markdown_text::char_at(line, j + 1).map_or(1, char::len_utf8);
            }
            b']' if any && at(line, j + 1) == b':' => {
                return format!("{}\\[{}", &line[..i], &line[i + 1..]);
            }
            0 | b'[' | b']' | b'\\' | b'\n' => return line.to_string(),
            _ => j += super::markdown_text::char_at(line, j).map_or(1, char::len_utf8),
        }
        any = true;
    }
}

/// A footnote citation at text[i] == "[": its id and the index after "]".
pub fn read_footnote_ref(text: &str, i: usize) -> Option<(String, usize)> {
    let from = i + 2;
    let close = text[from.min(text.len())..]
        .find([']', '\n'])
        .map(|c| c + from)?;
    if at(text, close) != b']' || close == from {
        return None;
    }
    Some((text[from..close].to_string(), close + 1))
}

/// The index past a dropped tag's body, same-name nesting counted.
pub fn skip_drop_content(body: &str, mut i: usize, name: &str, dead: &mut Dead) -> usize {
    let mut depth = 1;
    while i < body.len() && depth > 0 {
        let Some(open) = find_byte(body, b'<', i) else {
            return body.len();
        };
        let Some((tag, end)) = read_tag(body, open, Some(dead)) else {
            i = open + 1;
            continue;
        };
        let head = tag_head(tag);
        if head.name == name {
            if head.closing {
                depth -= 1;
            } else if !head.self_close {
                depth += 1;
            }
        }
        i = end;
    }
    i
}

/// Hides a definition's lines and records the span as dropped.
pub fn hide_definition(state: &mut Lines, from: usize, to: usize) {
    for i in from..=to {
        state.hidden.insert(i, Hidden::Line);
    }
    state.dropped.push((from, to));
}

/// The columns a line's indentation holds, up to four.
pub fn indent_width(line: &str, view: &View) -> usize {
    indentation_at(line, view, CODE_INDENT).1
}
