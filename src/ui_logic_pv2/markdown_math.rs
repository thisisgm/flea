//! Where a display formula starts and ends among the block reader's lines, which formulas a paragraph holds, and front matter.

use super::markdown_inline::{span_intervals, SpanKind};
use super::markdown_text::{at, blank, char_at, is_space, trim};

const INLINE_DELIMITER_LENGTH: usize = 1;
const DISPLAY_DELIMITER_LENGTH: usize = 2;

/// A display formula found at a line: its source, the prose after its closer, and the line it closes on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    pub source: String,
    pub tail: String,
    pub to: usize,
}

/// "$$x^2$$ tail" answers ("x^2", " tail", i); a lone "$$" opener reads on to its closer line within the paragraph.
pub fn display_at(lines: &[&str], i: usize, text: &str) -> Option<Display> {
    let mut indent = 0;
    while indent < 3 && at(text, indent) == b' ' {
        indent += 1;
    }
    let rest = text[indent..].strip_prefix("$$")?;
    if rest.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return None;
    }
    let mut source = String::new();
    let mut tail = String::new();
    let mut to = i;
    if let Some(close) = rest.find("$$") {
        source = trim(&rest[..close]).to_string();
        tail = rest[close + DISPLAY_DELIMITER_LENGTH..].to_string();
    } else {
        // The closer search stops at the paragraph's blank line, so no later opener scans these lines again.
        let end = (i + 1..lines.len())
            .take_while(|&at_line| !blank(lines[at_line]))
            .find(|&at_line| lines[at_line].contains("$$"));
        if let Some(end) = end {
            let end_at = lines[end].find("$$").unwrap_or(0);
            let mut all = vec![rest];
            all.extend_from_slice(&lines[i + 1..end]);
            all.push(&lines[end][..end_at]);
            source = trim(&all.join("\n")).to_string();
            tail = lines[end][end_at + DISPLAY_DELIMITER_LENGTH..].to_string();
            to = end;
        }
    }
    (!source.is_empty()).then_some(Display { source, tail, to })
}

/// A piece of a paragraph split at its display formulas: prose, or a formula's source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Text(String),
    Math(String),
}

/// "a $$x$$ b" answers [Text("a "), Math("x"), Text(" b")]; text with no display pair answers None.
pub fn split_display(text: &str) -> Option<Vec<Piece>> {
    if !text.contains("$$") {
        return None;
    }
    let mut pieces = Vec::new();
    let mut from = 0;
    for span in span_intervals(text) {
        if span.kind != SpanKind::Math || span.len != DISPLAY_DELIMITER_LENGTH {
            continue;
        }
        pieces.push(Piece::Text(text[from..span.from].to_string()));
        pieces.push(Piece::Math(
            trim(&text[span.from + DISPLAY_DELIMITER_LENGTH..span.to - DISPLAY_DELIMITER_LENGTH])
                .to_string(),
        ));
        from = span.to;
    }
    pieces.push(Piece::Text(text[from..].to_string()));
    (pieces.len() > 1).then_some(pieces)
}

/// "e $x^2$ and $y$" answers ["x^2", "y"]; display pairs and code spans hold none.
pub fn inline_sources(text: &str) -> Vec<String> {
    if !text.contains('$') {
        return Vec::new();
    }
    span_intervals(text)
        .into_iter()
        .filter(|span| span.kind == SpanKind::Math && span.len == INLINE_DELIMITER_LENGTH)
        .map(|span| trim(&text[span.from + 1..span.to - 1]).to_string())
        .collect()
}

// A front matter key line: a quoted key or a bare one, then ":" and a blank or the end.
fn key_line(line: &str) -> bool {
    let key_end = match char_at(line, 0) {
        Some(q @ ('"' | '\'')) => match line[1..].find(q) {
            Some(close) => close + 2,
            None => return false,
        },
        Some('-') => match char_at(line, 1) {
            Some(second) if !is_space(second) && second != ':' => {
                line.find(':').unwrap_or(line.len())
            }
            _ => return false,
        },
        Some(first) if !is_space(first) && first != '#' => line.find(':').unwrap_or(line.len()),
        _ => return false,
    };
    if at(line, key_end) != b':' {
        return false;
    }
    let tail = &line[key_end + 1..];
    tail.is_empty()
        || (matches!(at(tail, 0), b' ' | b'\t')
            && !tail.contains(['\n', '\r', '\u{2028}', '\u{2029}']))
}

// A front matter continuation: an indented value, a "-" item, or a "#" comment.
fn continuation_line(line: &str) -> bool {
    if line.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return false;
    }
    match at(line, 0) {
        b' ' | b'\t' => {
            let rest = line.trim_start_matches([' ', '\t']);
            rest.chars().next().is_some_and(|c| !is_space(c))
        }
        b'-' => line.len() == 1 || matches!(at(line, 1), b' ' | b'\t'),
        b'#' => true,
        _ => false,
    }
}

/// The closing line of YAML front matter at the top of a document: ["---", "title: A", "---"] answers 2; "---", "Foo", "---" answers None.
pub fn front_close_at(lines: &[&str]) -> Option<usize> {
    let mut end = 1;
    while end < lines.len() && lines[end] != "---" && lines[end] != "..." {
        end += 1;
    }
    if lines.first() != Some(&"---") || end >= lines.len() {
        return None;
    }
    let mut keyed = false;
    for line in &lines[1..end] {
        if blank(line) {
            continue;
        }
        if key_line(line) {
            keyed = true;
        } else if !keyed || !continuation_line(line) {
            return None;
        }
    }
    keyed.then_some(end)
}
