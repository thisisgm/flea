//! Leaf-block readers: tasks, fences, breaks, headings, tables, standalone images and alerts, each a pure function of its lines.

use super::markdown_model::{Block, CellAlign, Table};
use super::markdown_text::{at, is_space, trim};
use std::sync::atomic::{AtomicUsize, Ordering};

#[path = "markdown_leaf_image.rs"]
mod image;
pub use image::standalone_image;

const MAX_MARKER_INDENT: usize = 3;
const MAX_MARKER_DIGITS: usize = 9;
const MAX_HEADING_LEVEL: usize = 6;
/// Rows per table chunk and items per list chunk: a longer container is drawn as consecutive blocks, each built when seen.
pub const TABLE_CHUNK_ROWS: usize = 24;
pub const LIST_CHUNK_ITEMS: usize = 32;
// Columns an image holds in a cell's measure; its true width is only known once it decodes.
const IMAGE_COLUMNS: usize = 8;
const CELL_IMAGE: char = '\u{e000}';

/// The face a task box is pinned to, so the open and the done box draw from one text font.
pub const TASK_BOX_FACE: &str = "Noto Sans Symbols 2";
pub const TASK_OPEN: char = '\u{2610}';
pub const TASK_DONE: char = '\u{2611}';

// True when the text holds a character that ends a line for ".": newline, carriage return or a line separator.
fn breaks_line(text: &str) -> bool {
    text.contains(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// "[x] done" draws U+2611 and "[ ] pending" U+2610; any other text is unchanged.
pub fn task_text(text: &str) -> String {
    let bytes = text.as_bytes();
    if bytes.len() >= 4
        && bytes[0] == b'['
        && matches!(bytes[1], b' ' | b'x' | b'X')
        && &bytes[2..4] == b"] "
    {
        let rest = &text[4..];
        if !breaks_line(rest) {
            let mark = if bytes[1] == b' ' {
                TASK_OPEN
            } else {
                TASK_DONE
            };
            return format!("{mark} {rest}");
        }
    }
    text.to_string()
}

/// A leading task box set in the task face: "\u{2611} done" answers '<font face="Noto Sans Symbols 2">\u{2611}</font> done'.
pub fn pin_task_box(text: &str) -> String {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(mark @ (TASK_OPEN | TASK_DONE)), Some(' ')) => {
            format!(
                "<font face=\"{TASK_BOX_FACE}\">{mark}</font>{}",
                &text[mark.len_utf8()..]
            )
        }
        _ => text.to_string(),
    }
}

/// True for a thematic break line such as "  * * *".
pub fn is_thematic(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < MAX_MARKER_INDENT && at(line, i) == b' ' {
        i += 1;
    }
    let mark = at(line, i);
    if !matches!(mark, b'*' | b'_' | b'-') {
        return false;
    }
    let mut count = 0;
    for &b in &bytes[i..] {
        if b == mark {
            count += 1;
        } else if b != b' ' && b != b'\t' {
            return false;
        }
    }
    count >= 3
}

/// An ATX heading's level and text: "## Second level ##" answers (2, "Second level"); "#hashtag" answers None.
pub fn atx_heading(line: &str) -> Option<(u8, &str)> {
    let s = line.as_bytes();
    let mut i = 0;
    while i < MAX_MARKER_INDENT && at(line, i) == b' ' {
        i += 1;
    }
    let from = i;
    while i < s.len() && s[i] == b'#' {
        i += 1;
    }
    let level = i - from;
    if !(1..=MAX_HEADING_LEVEL).contains(&level) || (i < s.len() && s[i] != b' ' && s[i] != b'\t') {
        return None;
    }
    let blank = |b: u8| b == b' ' || b == b'\t';
    let mut end = s.len();
    while end > i && blank(s[end - 1]) {
        end -= 1;
    }
    let mut close = end;
    while close > i && s[close - 1] == b'#' {
        close -= 1;
    }
    if close < end && (close == i || blank(s[close - 1])) {
        end = close;
    }
    while end > i && blank(s[end - 1]) {
        end -= 1;
    }
    while i < end && blank(s[i]) {
        i += 1;
    }
    Some((level as u8, &line[i..end]))
}

// "1. Intro" answers the digits' length; a marker needs "." or ")" then a blank or the end.
fn ordered_marker(text: &str) -> Option<usize> {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let after = at(text, digits + 1);
    ((1..=MAX_MARKER_DIGITS).contains(&digits)
        && matches!(at(text, digits), b'.' | b')')
        && (after == b' ' || after == b'\t' || digits + 1 == text.len()))
    .then_some(digits)
}

// "- x", ">", "## x", "```" and "~~~" open a block at the start of a heading's text.
fn heading_marker(t: &str) -> bool {
    let first = at(t, 0);
    let after = at(t, 1);
    if matches!(first, b'-' | b'+' | b'*') && (after == b' ' || after == b'\t' || t.len() == 1) {
        return true;
    }
    if first == b'>' || t.starts_with("```") || t.starts_with("~~~") {
        return true;
    }
    let hashes = t.bytes().take_while(|b| *b == b'#').count();
    (1..=MAX_HEADING_LEVEL).contains(&hashes)
        && (hashes == t.len() || matches!(at(t, hashes), b' ' | b'\t'))
}

/// Heading text that cannot reopen a block: "1. Intro" answers "1\\. Intro", "- item" answers "\\- item".
pub fn heading_safe(text: &str) -> String {
    if let Some(digits) = ordered_marker(text) {
        return format!("{}\\{}", &text[..digits], &text[digits..]);
    }
    if heading_marker(text) || is_thematic(text) {
        format!("\\{text}")
    } else {
        text.to_string()
    }
}

/// True for a setext underline, "=" or "-" runs with at most three spaces before.
pub fn is_setext(line: &str) -> bool {
    let mut i = 0;
    while i < MAX_MARKER_INDENT && at(line, i) == b' ' {
        i += 1;
    }
    let mark = at(line, i);
    if mark != b'=' && mark != b'-' {
        return false;
    }
    let run = line[i..].bytes().take_while(|b| *b == mark).count();
    line[i + run..].bytes().all(|b| b == b' ' || b == b'\t')
}

/// The heading level a setext underline gives: "===" level 1, "---" level 2.
pub fn setext_level(line: &str) -> u8 {
    if trim(line).starts_with('=') {
        1
    } else {
        2
    }
}

/// A fence opener: its mark, run length, indent and info string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fence {
    pub tick: u8,
    pub len: usize,
    pub indent: usize,
    pub info: String,
}

/// "```js" opens a backtick fence with info "js"; a backtick fence's info may hold no backtick.
pub fn fence_open(line: &str) -> Option<Fence> {
    let mut i = 0;
    while i < MAX_MARKER_INDENT && at(line, i) == b' ' {
        i += 1;
    }
    let tick = at(line, i);
    if tick != b'`' && tick != b'~' {
        return None;
    }
    let len = line[i..].bytes().take_while(|b| *b == tick).count();
    if len < 3 {
        return None;
    }
    let mut j = i + len;
    while at(line, j) == b' ' {
        j += 1;
    }
    let info = &line[j..];
    if breaks_line(info) || (tick == b'`' && info.contains('`')) {
        return None;
    }
    Some(Fence {
        tick,
        len,
        indent: i,
        info: info.trim_end_matches(is_space).to_string(),
    })
}

/// A delimiter row's alignments: dashes with optional edge colons, or None when any cell is something else.
pub fn delim_aligns(line: &str) -> Option<Vec<CellAlign>> {
    let text = trim(line);
    let text = text.strip_prefix('|').unwrap_or(text);
    let text = text.strip_suffix('|').unwrap_or(text);
    text.split('|')
        .map(|cell| {
            let cell = trim(cell);
            let left = cell.starts_with(':');
            let body = cell.strip_prefix(':').unwrap_or(cell);
            let right = body.ends_with(':');
            let body = body.strip_suffix(':').unwrap_or(body);
            if body.is_empty() || !body.bytes().all(|b| b == b'-') {
                return None;
            }
            Some(match (left, right) {
                (true, true) => CellAlign::Center,
                (_, true) => CellAlign::Right,
                _ => CellAlign::Left,
            })
        })
        .collect()
}

/// A row's cells: "| a | b \\|" keeps the final pipe as cell text; other backslash pairs reach the inline scanner.
pub fn split_row(line: &str) -> Vec<String> {
    let text = trim(line);
    let text = text.strip_prefix('|').unwrap_or(text);
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        let next = chars.peek().map(|(_, c)| *c);
        if ch == '\\' && next.is_some_and(|c| super::markdown_escape::is_ascii_punct(c as u32)) {
            let next = next.unwrap_or(' ');
            if next != '|' {
                cell.push('\\');
            }
            cell.push(next);
            chars.next();
        } else if ch == '|' {
            if i + 1 == text.len() {
                break;
            }
            cells.push(std::mem::take(&mut cell));
        } else {
            cell.push(ch);
        }
    }
    cells.push(cell);
    cells
        .into_iter()
        .map(|cell| trim(&cell).to_string())
        .collect()
}

#[path = "markdown_leaf_cells.rs"]
mod cells;
pub use cells::{cell_extent, table_block, Extent};

// The tables chunked so far, which numbers each one's chunks.
static CHUNKED_TABLES: AtomicUsize = AtomicUsize::new(0);

/// A long table as consecutive blocks: the header stays on the first, the rest are joined, all keyed to scroll as one.
pub fn chunk_table(table: Table) -> Vec<Block> {
    if table.rows.len() <= TABLE_CHUNK_ROWS {
        return vec![Block::Table(table)];
    }
    let key = CHUNKED_TABLES.fetch_add(1, Ordering::Relaxed) + 1;
    let total = table.rows.len();
    table
        .rows
        .chunks(TABLE_CHUNK_ROWS)
        .enumerate()
        .map(|(n, rows)| {
            let at = n * TABLE_CHUNK_ROWS;
            Block::Table(Table {
                head: if at == 0 {
                    table.head.clone()
                } else {
                    Vec::new()
                },
                aligns: table.aligns.clone(),
                rows: rows.to_vec(),
                cols: table.cols,
                measure: table.measure.clone(),
                pictures: table.pictures.clone(),
                weights: table.weights.clone(),
                words: table.words.clone(),
                joined: at > 0,
                key: Some(key),
                last: Some(at + TABLE_CHUNK_ROWS >= total),
            })
        })
        .collect()
}

/// A GFM alert's title line: "[!NOTE] Remember this" answers "**Note** Remember this".
pub fn alert_title(line: &str) -> Option<String> {
    let rest = line.strip_prefix("[!")?;
    let close = rest.find(']')?;
    let name = &rest[..close];
    let known = ["NOTE", "TIP", "IMPORTANT", "WARNING", "CAUTION"];
    if !known.iter().any(|k| k.eq_ignore_ascii_case(name)) {
        return None;
    }
    let tail = rest[close + 1..].trim_start_matches(is_space);
    if breaks_line(tail) {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    let title = format!("{}{}", lower[..1].to_ascii_uppercase(), &lower[1..]);
    Some(if tail.is_empty() {
        format!("**{title}**")
    } else {
        format!("**{title}** {tail}")
    })
}
