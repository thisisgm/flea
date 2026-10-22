//! Where the CSV table's columns draw: the preview column's fitted boxes, Quick Look's grid and the row numbers.

/// A drawn column box: its left edge, its width and its right alignment.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub x: f64,
    pub w: f64,
    pub right: bool,
}

/// The column boxes that fit the frame: the first column flexes to fill it, or narrows to its header when the columns do not fit.
pub fn fit(widths: &[f64], heads: &[f64], rights: &[bool], gap: f64, avail: f64) -> Vec<Slot> {
    let mut out = Vec::new();
    if widths.is_empty() {
        return out;
    }
    let n = widths.len();
    let total = widths.iter().sum::<f64>() + gap * (n as f64 - 1.0);
    let first = if total <= avail {
        widths[0] + (avail - total)
    } else {
        widths[0].min(heads[0])
    };
    let mut x = 0.0;
    let mut c = 0;
    while c < n && x < avail {
        let w = if c == 0 { first } else { widths[c] };
        out.push(Slot { x, w: w.min(avail - x), right: rights[c] });
        x += w + gap;
        c += 1;
    }
    out
}

/// The Quick Look boxes: each column is its widest cell plus padding on both sides, after the row numbers.
pub fn grid(widths: &[f64], rights: &[bool], number_width: f64, pad: f64) -> Vec<Slot> {
    let mut out = Vec::new();
    let mut x = number_width;
    for (c, width) in widths.iter().enumerate() {
        let w = width + 2.0 * pad;
        out.push(Slot { x, w, right: rights[c] });
        x += w;
    }
    out
}

/// The data rows after the header, never below zero.
pub fn data_rows(total: usize) -> usize {
    total.saturating_sub(1)
}

/// The data row number is its record ordinal less the header, never below zero.
pub fn row_number(record: usize) -> usize {
    record.saturating_sub(1)
}

#[cfg(test)]
#[path = "csv_layout_tests.rs"]
mod tests;
