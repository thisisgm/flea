//! The Source view's rows: a numbered gutter, and each line's runs on one line ending in an ellipsis or wrapped at its spaces.

use super::markdown_flow::{Face, Measure};

/// The character that ends a line cut at the view's edge.
pub const ELLIPSIS: &str = "\u{2026}";

/// The gutter's width in digit advances: the widest number shown, at least two digits, and one more.
pub fn gutter_columns(shown: usize) -> usize {
    shown.to_string().len().max(2) + 1
}

/// The lines a text holds: none for an empty text, and a trailing line end opens no line of its own.
pub fn row_count(text: &str, starts: &[usize]) -> usize {
    if text.is_empty() {
        0
    } else {
        starts.len()
    }
}

/// Line `n` (1-based) of a text as one plain run, without its line end or a carriage return before it.
pub fn plain_row(text: &str, starts: &[usize], n: usize) -> Option<Vec<(i64, String)>> {
    let from = *starts.get(n.checked_sub(1)?)?;
    let to = starts.get(n).map_or(text.len(), |next| next - 1);
    let line = text.get(from..to.max(from))?;
    let line = line.strip_suffix('\n').unwrap_or(line);
    Some(vec![(
        0,
        line.strip_suffix('\r').unwrap_or(line).to_string(),
    )])
}

/// One piece of a drawn line: where it starts, its ink role and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub start: usize,
    pub len: usize,
    pub x: f32,
    pub role: i64,
    pub text: String,
}

// One character of a line with its role, its advance and whether a wrap may follow it.
struct Cell {
    byte: usize,
    c: char,
    role: i64,
    width: f32,
    breaks: bool,
}

fn cells(runs: &[(i64, String)], px: f32, measure: &mut dyn Measure) -> Vec<Cell> {
    let line: Vec<(char, i64)> = runs
        .iter()
        .flat_map(|(role, text)| text.chars().map(move |c| (c, *role)))
        .collect();
    let mut out = Vec::with_capacity(line.len());
    let mut byte = 0;
    for (i, &(c, role)) in line.iter().enumerate() {
        // Only one space between two other characters may wrap; a run of spaces holds together.
        let lone =
            c == ' ' && i > 0 && i + 1 < line.len() && line[i - 1].0 != ' ' && line[i + 1].0 != ' ';
        let width = measure.width(c.encode_utf8(&mut [0; 4]), Face::Regular, px);
        out.push(Cell {
            byte,
            c,
            role,
            width,
            breaks: lone,
        });
        byte += c.len_utf8();
    }
    out
}

// The cells as pieces from x 0, one per run of a role.
fn pieces(cells: &[Cell]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut x = 0.0;
    for cell in cells {
        match out.last_mut() {
            Some(last) if last.role == cell.role => {
                last.text.push(cell.c);
                last.len += cell.c.len_utf8();
            }
            _ => out.push(Piece {
                start: cell.byte,
                len: cell.c.len_utf8(),
                x,
                role: cell.role,
                text: cell.c.to_string(),
            }),
        }
        x += cell.width;
    }
    out
}

/// A line's runs, each with every tab already filled, as the lines the row draws in `width`.
/// Unwrapped, a line too long ends in an ellipsis in the ink it is cut in; wrapped, it breaks after a lone space,
/// and a word wider than the view breaks anywhere.
pub fn row_lines(
    runs: &[(i64, String)],
    width: f32,
    wrap: bool,
    px: f32,
    measure: &mut dyn Measure,
) -> Vec<Vec<Piece>> {
    let cells = cells(runs, px, measure);
    let total: f32 = cells.iter().map(|cell| cell.width).sum();
    if total <= width {
        return vec![pieces(&cells)];
    }
    if !wrap {
        let ellipsis = measure.width(ELLIPSIS, Face::Regular, px);
        let mut used = 0.0;
        let kept = cells.iter().take_while(|cell| {
            used += cell.width;
            used <= width - ellipsis
        });
        let kept = kept.count();
        let role = cells.get(kept).or(cells.last()).map_or(0, |cell| cell.role);
        let mut line = pieces(&cells[..kept]);
        let x = cells[..kept].iter().map(|cell| cell.width).sum();
        match line.last_mut() {
            Some(last) if last.role == role => last.text.push_str(ELLIPSIS),
            _ => line.push(Piece {
                start: cells.get(kept).map_or(0, |cell| cell.byte),
                len: 0,
                x,
                role,
                text: ELLIPSIS.to_string(),
            }),
        }
        return vec![line];
    }
    let mut lines = Vec::new();
    let mut start = 0;
    while start < cells.len() {
        let mut used = 0.0;
        let mut end = start;
        let mut last_break = None;
        while end < cells.len() {
            let cell = &cells[end];
            // A lone space at the end of a line hangs past the edge.
            if !cell.breaks && used + cell.width > width && end > start {
                break;
            }
            used += cell.width;
            end += 1;
            if cell.breaks {
                last_break = Some(end);
            }
        }
        let cut = if end >= cells.len() {
            end
        } else {
            last_break.filter(|at| *at > start).unwrap_or(end)
        };
        let mut line = pieces(&cells[start..cut]);
        if let Some(last) = line
            .last_mut()
            .filter(|_| cut < cells.len() && cells[cut - 1].breaks)
        {
            last.text.pop();
            last.len -= 1;
            if last.text.is_empty() {
                line.pop();
            }
        }
        lines.push(line);
        start = cut;
    }
    lines
}

#[cfg(test)]
#[path = "markdown_code_rows_tests.rs"]
mod tests;
