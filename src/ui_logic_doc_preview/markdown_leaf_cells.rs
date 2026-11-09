//! A table's cells as columns of text: each cell's drawn extent, and the measures every chunk of the table shares.

use super::super::markdown_escape::CHIP_PAD;
use super::super::markdown_model::{CellAlign, Table};
use super::super::markdown_text::{at, is_space};
use super::{CELL_IMAGE, IMAGE_COLUMNS};

/// A cell's widest line of text and of text with images, and its longest unbroken word, in columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Extent {
    pub text: usize,
    pub image: usize,
    pub word: usize,
}

fn lower_starts(text: &str, i: usize, prefix: &str) -> bool {
    text.len() >= i + prefix.len()
        && text.as_bytes()[i..i + prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

// Each `<br\s*\/?>`, any case, as a newline.
fn breaks_as_newlines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if let Some(end) = super::super::markdown_html_image::break_at(text, i) {
            out.push('\n');
            i = end;
            continue;
        }
        let c = super::super::markdown_text::char_at(text, i).unwrap_or(' ');
        out.push(c);
        i += c.len_utf8();
    }
    out
}

// Each image as a Markdown "![..](..)" or a raw "<img ...>" tag, as one private-use mark.
fn images_as_marks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'!' && at(text, i + 1) == b'[' {
            if let Some(close) = text[i + 2..].find(']').map(|c| c + i + 2) {
                if at(text, close + 1) == b'(' {
                    if let Some(end) = text[close + 2..].find(')').map(|e| e + close + 2) {
                        out.push(CELL_IMAGE);
                        i = end + 1;
                        continue;
                    }
                }
            }
        }
        if bytes[i] == b'<'
            && lower_starts(text, i + 1, "img")
            && !super::super::markdown_text::is_word(at(text, i + 4))
        {
            if let Some(end) = text[i + 4..].find('>').map(|e| e + i + 4) {
                out.push(CELL_IMAGE);
                i = end + 1;
                continue;
            }
        }
        let c = super::super::markdown_text::char_at(text, i).unwrap_or(' ');
        out.push(c);
        i += c.len_utf8();
    }
    out
}

// Every tag, its double-quoted values holding any ">", removed; a tag whose quote never closes stays.
fn without_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut plain = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let mut j = i + 1;
            let mut quoted = false;
            let mut end = None;
            while j < bytes.len() {
                match bytes[j] {
                    b'"' => quoted = !quoted,
                    b'>' if !quoted => {
                        end = Some(j);
                        break;
                    }
                    _ => {}
                }
                j += 1;
            }
            if let Some(end) = end {
                out.push_str(&text[plain..i]);
                i = end + 1;
                plain = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&text[plain..]);
    out
}

// Numeric references back to their characters, then the four common named ones as one column each.
fn references_decoded(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut i = 0;
    let mut plain = 0;
    while i < text.len() {
        if at(text, i) == b'&' && at(text, i + 1) == b'#' {
            let digits = text[i + 2..].bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && at(text, i + 2 + digits) == b';' {
                decoded.push_str(&text[plain..i]);
                let code: u64 = text[i + 2..i + 2 + digits].parse().unwrap_or(0xfffd);
                decoded.push(char::from_u32((code & 0xffff) as u32).unwrap_or('\u{fffd}'));
                i += 3 + digits;
                plain = i;
                continue;
            }
        }
        i += 1;
    }
    decoded.push_str(&text[plain..]);
    let mut out = String::with_capacity(decoded.len());
    let (mut i, mut plain) = (0, 0);
    while i < decoded.len() {
        if at(&decoded, i) == b'&' {
            if let Some(name) = ["amp;", "lt;", "gt;", "quot;"]
                .iter()
                .find(|n| decoded[i + 1..].starts_with(*n))
            {
                out.push_str(&decoded[plain..i]);
                out.push('?');
                i += 1 + name.len();
                plain = i;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&decoded[plain..]);
    out
}

fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115f | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff
            | 0xfe30..=0xfe6f | 0xff00..=0xff60 | 0xffe0..=0xffe6)
}

/// A cell's drawn extent: a wide glyph counts 2, a joiner, a variation selector and a chip's pad 0, a break starts a line, an image counts only in image.
pub fn cell_extent(cell: &str) -> Extent {
    let plain = cell.replace(CHIP_PAD, "");
    let plain = images_as_marks(&breaks_as_newlines(&plain));
    let plain: String = without_tags(&plain)
        .chars()
        .filter(|c| !matches!(c, '*' | '_' | '~' | '`'))
        .collect();
    let plain = references_decoded(&plain);
    let mut out = Extent::default();
    for line in plain.split('\n') {
        let (mut text, mut image, mut word) = (0, 0, 0);
        let mut joined = false;
        for g in line.chars() {
            let space = is_space(g);
            let picture = g == CELL_IMAGE;
            let cols = if joined || g == '\u{200d}' || g == '\u{fe0f}' {
                0
            } else if picture {
                IMAGE_COLUMNS
            } else if space {
                1
            } else if g as u32 > 0xffff || is_wide(g) {
                2
            } else {
                1
            };
            joined = g == '\u{200d}';
            image += cols;
            if !picture {
                text += cols;
            }
            word = if space { 0 } else { word + cols };
            out.word = out.word.max(word);
        }
        out.text = out.text.max(text);
        out.image = out.image.max(image);
    }
    out
}

// The cell with the most columns of one measure, among cells holding an image when images_only.
fn widest<'c>(
    cells: &[&'c str],
    drawn: &[Extent],
    key: fn(&Extent) -> usize,
    images_only: bool,
) -> &'c str {
    let mut best = "";
    let mut best_columns = 0;
    for (cell, extent) in cells.iter().zip(drawn) {
        if key(extent) > best_columns && (!images_only || extent.image > extent.text) {
            best = cell;
            best_columns = key(extent);
        }
    }
    best
}

/// A table as data: measure is each column's widest cell so chunks share widths, pictures its widest image cell, words its longest run.
pub fn table_block(
    head: &[String],
    aligns: Vec<CellAlign>,
    rows: &[Vec<String>],
    inline_of: &mut dyn FnMut(&str) -> String,
) -> Table {
    let cols = head.len();
    let shown_head: Vec<String> = head.iter().map(|cell| inline_of(cell)).collect();
    // GFM: a row's cells beyond the header's are dropped, and a short row is padded with empty cells.
    let shown_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|cells| {
            let mut row: Vec<String> = cells
                .iter()
                .take(cols)
                .map(|cell| inline_of(cell))
                .collect();
            row.resize(cols, String::new());
            row
        })
        .collect();
    let mut table = Table {
        cols,
        aligns,
        ..Table::default()
    };
    for c in 0..cols {
        let mut column: Vec<&str> = vec![shown_head.get(c).map_or("", String::as_str)];
        column.extend(
            shown_rows
                .iter()
                .map(|row| row.get(c).map_or("", String::as_str)),
        );
        let drawn: Vec<Extent> = column.iter().map(|cell| cell_extent(cell)).collect();
        let measure = widest(&column, &drawn, |e| e.text, false).to_string();
        table
            .pictures
            .push(widest(&column, &drawn, |e| e.image, true).to_string());
        table.weights.push(cell_extent(&measure).text);
        table
            .words
            .push(cell_extent(widest(&column, &drawn, |e| e.word, false)).word);
        table.measure.push(measure);
    }
    table.head = shown_head;
    table.rows = shown_rows;
    table
}
