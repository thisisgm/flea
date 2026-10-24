//! The line-level readers of the inline scan: hard breaks, thematic break lines and quote marks inside items.

use super::markdown_emph::process;
use super::markdown_leaf::is_thematic;
use super::markdown_piece::{Delim, Piece, HARD_BREAK};
use super::markdown_text::{at, find_byte, is_space};

const HARD_BREAK_SPACES: usize = 2;
const MAX_MARKER_LENGTH: usize = 10;
const RULE_LINE_INDENT: usize = 3;

/// The emphasis marks of the current paragraph start at bottom; a blank line moves it past every mark so far.
pub struct Delims {
    pub list: Vec<Delim>,
    pub bottom: usize,
}

/// Reads a line ending at body[i] ("\n" or "\\\n"): a hard break, a soft one, or a paragraph end; answers the index after it.
pub fn line_break(
    body: &str,
    i: usize,
    out: &mut Vec<Piece>,
    delims: &mut Delims,
    end_paragraph: &mut dyn FnMut(),
) -> usize {
    let slash = at(body, i) == b'\\';
    let mut next = if slash { i + 2 } else { i + 1 };
    let mut spaces = 0;
    while !slash && out.last().is_some_and(|piece| piece.is(" ")) {
        out.pop();
        spaces += 1;
    }
    while next < body.len() && matches!(at(body, next), b' ' | b'\t') {
        next += 1;
    }
    // A blank line ends the paragraph, and no emphasis mark pairs and no bracket pair closes across it.
    if next < body.len() && at(body, next) == b'\n' {
        let bottom = delims.bottom;
        process(&mut delims.list, bottom);
        delims.bottom = delims.list.len();
        end_paragraph();
    }
    if next < body.len() && at(body, next) != b'\n' && (slash || spaces >= HARD_BREAK_SPACES) {
        out.push(Piece::Lit(HARD_BREAK));
        return next;
    }
    if slash {
        out.push(Piece::Ch('\\'));
    }
    out.push(Piece::Ch('\n'));
    if slash {
        i + 2
    } else {
        i + 1
    }
}

/// True when the mark at body[at] sits on a thematic break line, which the renderer draws as a rule.
pub fn is_rule_line(body: &str, mark: usize) -> bool {
    let mut from = mark;
    while from > 0 && mark - from < RULE_LINE_INDENT && at(body, from - 1) == b' ' {
        from -= 1;
    }
    if from > 0 && at(body, from - 1) != b'\n' {
        return false;
    }
    let end = find_byte(body, b'\n', mark).unwrap_or(body.len());
    is_thematic(&body[from..end])
}

// The last index at or before at that is no space or tab.
fn skip_blanks_back(body: &str, at_byte: isize) -> isize {
    let mut p = at_byte;
    while p >= 0 && matches!(at(body, p as usize), b' ' | b'\t') {
        p -= 1;
    }
    p
}

fn is_list_mark(text: &str) -> bool {
    if matches!(text, "-" | "+" | "*") {
        return true;
    }
    let digits = text.len().saturating_sub(1);
    (1..=9).contains(&digits)
        && text.as_bytes()[..digits].iter().all(u8::is_ascii_digit)
        && matches!(at(text, digits), b'.' | b')')
}

/// True when the ">" at body[at] is a quote mark inside an item, as in "- > q"; last is the previous mark's index.
pub fn quote_mark_at(body: &str, mark: usize, last: Option<usize>) -> bool {
    let is_last = |p: isize| last.is_some_and(|l| l as isize == p);
    let p = skip_blanks_back(body, mark as isize - 1);
    if p < 0 || at(body, p as usize) == b'\n' || is_last(p) {
        return true;
    }
    // A word ending in a non-ASCII character is never a list marker.
    if at(body, p as usize) >= 0x80 {
        return false;
    }
    let mut from = p;
    while from > 0 {
        let prev = super::markdown_text::char_before(body, from as usize).unwrap_or(' ');
        if is_space(prev) {
            break;
        }
        from -= prev.len_utf8() as isize;
    }
    if p - from >= MAX_MARKER_LENGTH as isize
        || p == mark as isize - 1
        || !is_list_mark(&body[from as usize..=p as usize])
    {
        return false;
    }
    let before = skip_blanks_back(body, from - 1);
    before < 0 || at(body, before as usize) == b'\n' || is_last(before)
}
