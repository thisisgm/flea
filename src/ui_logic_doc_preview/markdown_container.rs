//! Container prefix readers over byte offsets and absolute tab columns, without copying the rest of the line.

use super::markdown_leaf::{atx_heading, fence_open, is_thematic};
use super::markdown_text::{at, blank};

pub const CODE_INDENT: usize = 4;
const MAX_MARKER_INDENT: usize = 3;
const MAX_MARKER_DIGITS: usize = 9;
const MAX_GAP_SCAN: usize = CODE_INDENT + 1;
const MIN_MARKER_GAP: usize = 1;

/// Where a line is read from: the byte, the columns of a split tab still owed as spaces, and the column reached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub at: usize,
    pub padding: usize,
    pub column: usize,
}

/// The indentation at the view up to limit columns: the byte it ends at and its width; a tab advances to its stop.
pub fn indentation_at(line: &str, view: &View, limit: usize) -> (usize, usize) {
    let mut at_byte = view.at;
    let mut width = view.padding;
    while at_byte < line.len() && width < limit {
        let c = at(line, at_byte);
        if c != b' ' && c != b'\t' {
            break;
        }
        width += if c == b'\t' {
            CODE_INDENT - (view.column + width) % CODE_INDENT
        } else {
            1
        };
        at_byte += 1;
    }
    (at_byte, width)
}

/// Consumes wanted columns of indentation, keeping the rest of a split tab as padding.
pub fn take_indent(line: &str, view: &mut View, wanted: usize) {
    let mut width = view.padding.min(wanted);
    view.padding -= width;
    while view.at < line.len() && width < wanted {
        let c = at(line, view.at);
        if c != b' ' && c != b'\t' {
            break;
        }
        let step = if c == b'\t' {
            CODE_INDENT - (view.column + width) % CODE_INDENT
        } else {
            1
        };
        view.at += 1;
        width += step;
    }
    view.padding += width.saturating_sub(wanted);
    view.column += width.min(wanted);
}

/// The indent before a quote mark at the view, or None when no quote opens there.
pub fn quote_at(line: &str, view: &View) -> Option<usize> {
    let (end, width) = indentation_at(line, view, CODE_INDENT);
    (width <= MAX_MARKER_INDENT && at(line, end) == b'>').then_some(width)
}

/// Consumes a quote mark and the one column of space after it.
pub fn take_quote(line: &str, view: &mut View, indent: usize) {
    take_indent(line, view, indent);
    view.at += 1;
    view.column += 1;
    if matches!(at(line, view.at), b' ' | b'\t') {
        take_indent(line, view, 1);
    }
}

/// A list marker at the view: its indent, kind, mark, start number and the columns its content starts at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    pub indent: usize,
    pub ordered: bool,
    pub mark: u8,
    pub start: u64,
    pub content_col: usize,
    pub marker_end: usize,
    pub marker_column: usize,
    pub gap: usize,
}

/// The list marker at the view: "10.\ttext" starts content at column four, and five gap columns leave indented code.
pub fn list_at(line: &str, view: &View) -> Option<Marker> {
    let (end, indent) = indentation_at(line, view, CODE_INDENT);
    if indent > MAX_MARKER_INDENT {
        return None;
    }
    let start = end;
    let mut i = end;
    let c = at(line, i);
    let ordered = c.is_ascii_digit();
    if ordered {
        while i - start < MAX_MARKER_DIGITS && at(line, i).is_ascii_digit() {
            i += 1;
        }
        if !matches!(at(line, i), b'.' | b')') {
            return None;
        }
    } else if !matches!(c, b'-' | b'*' | b'+') {
        return None;
    }
    let mark = at(line, i);
    i += 1;
    if i < line.len() && !matches!(at(line, i), b' ' | b'\t') {
        return None;
    }
    let marker_width = i - start;
    let gap_view = View {
        at: i,
        padding: 0,
        column: view.column + indent + marker_width,
    };
    let gap = if blank(&line[i.min(line.len())..]) {
        MIN_MARKER_GAP
    } else {
        indentation_at(line, &gap_view, MAX_GAP_SCAN).1
    };
    let consumed = if gap > CODE_INDENT {
        MIN_MARKER_GAP
    } else {
        gap
    };
    Some(Marker {
        indent,
        ordered,
        mark,
        start: if ordered {
            line[start..i - 1].parse().unwrap_or(0)
        } else {
            0
        },
        content_col: indent + marker_width + consumed,
        marker_end: i,
        marker_column: gap_view.column,
        gap: consumed,
    })
}

/// Consumes a list marker and its gap.
pub fn take_list(line: &str, view: &mut View, marker: &Marker) {
    view.at = marker.marker_end;
    view.padding = 0;
    view.column = marker.marker_column;
    take_indent(line, view, marker.gap);
}

/// The text with its leading tabs expanded to their stops from column: "\tbar" at column 2 answers "  bar".
pub fn expand_lead(text: &str, column: usize) -> String {
    let mut out = String::with_capacity(text.len() + 4);
    let mut col = column;
    let mut i = 0;
    while matches!(at(text, i), b' ' | b'\t') {
        let step = if at(text, i) == b'\t' {
            CODE_INDENT - col % CODE_INDENT
        } else {
            1
        };
        out.extend(std::iter::repeat(' ').take(step));
        col += step;
        i += 1;
    }
    out.push_str(&text[i..]);
    out
}

/// The line's text from the view, its owed padding as spaces.
pub fn text_at(line: &str, view: &View) -> String {
    let mut out = " ".repeat(view.padding);
    out.push_str(&line[view.at.min(line.len())..]);
    out
}

/// A list marker read from the start of a line, with the text after it.
pub fn read_list_marker(line: &str) -> Option<(Marker, String)> {
    let mut view = View::default();
    let marker = list_at(line, &view)?;
    take_list(line, &mut view, &marker);
    Some((marker, text_at(line, &view)))
}

/// The text with at most width leading spaces removed, as a fence opened at that indent drops from each body line.
pub fn unindent(text: &str, width: usize) -> &str {
    let spaces = text.bytes().take(width).take_while(|b| *b == b' ').count();
    &text[spaces..]
}

/// True when a line starts a block a table stops before: a quote, a list item, a fence, a heading or a rule.
pub fn starts_block(line: &str) -> bool {
    let view = View::default();
    quote_at(line, &view).is_some()
        || list_at(line, &view).is_some()
        || fence_open(line).is_some()
        || atx_heading(line).is_some()
        || is_thematic(line)
}
