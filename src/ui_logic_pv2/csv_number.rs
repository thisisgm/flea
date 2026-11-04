//! Which CSV cells read as numbers, the way JavaScript's Number reads them, and so which columns align right.

/// A non-empty cell that parses as a finite number once its spaces are trimmed.
pub fn is_number(text: &str) -> bool {
    let value = text.trim();
    if value.is_empty() {
        return false;
    }
    matches!(js_number(value), Some(n) if n.is_finite())
}

/// The value JavaScript Number gives a trimmed, non-empty string; None where it gives NaN.
fn js_number(value: &str) -> Option<f64> {
    let radix: u32 = match value.get(..2) {
        Some("0x") | Some("0X") => 16,
        Some("0o") | Some("0O") => 8,
        Some("0b") | Some("0B") => 2,
        _ => 0,
    };
    if radix != 0 {
        let digits = &value[2..];
        if digits.is_empty() {
            return None;
        }
        let mut total = 0.0;
        for ch in digits.chars() {
            total = total * f64::from(radix) + f64::from(ch.to_digit(radix)?);
        }
        return Some(total);
    }
    let body = match value.as_bytes().first() {
        Some(b'+') | Some(b'-') => &value[1..],
        _ => value,
    };
    if body == "Infinity" {
        return Some(f64::INFINITY);
    }
    if !is_decimal_literal(body) {
        return None;
    }
    value.parse::<f64>().ok()
}

/// Whether the text is an unsigned decimal literal: digits, an optional fraction and an optional exponent.
fn is_decimal_literal(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return false;
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let exponent_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == exponent_start {
            return false;
        }
    }
    i == bytes.len()
}

/// Whether each column is right-aligned: every non-empty body cell in it is a number. Row 0 is the header and is not read.
/// True when the column's non-empty values all read as numbers and at least one is non-empty.
pub fn numeric_column<'a>(values: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = false;
    for text in values {
        if text.trim().is_empty() {
            continue;
        }
        seen = true;
        if !is_number(text) {
            return false;
        }
    }
    seen
}

pub fn numeric_columns(rows: &[Vec<String>], columns: usize) -> Vec<bool> {
    (0..columns)
        .map(|c| {
            numeric_column(
                rows.iter()
                    .skip(1)
                    .map(|row| row.get(c).map_or("", String::as_str)),
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "csv_number_tests.rs"]
mod tests;
