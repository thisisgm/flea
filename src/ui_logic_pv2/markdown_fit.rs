//! How a table shares its width, how an inline picture fits its line, and the box a formula's picture holds around the baseline.

/// fit([100, 400], [50, 100], 300, 10) is [71, 228]: each column keeps its longest word and the rest goes by want.
/// A table that fits keeps its natural widths; past that no column falls under its longest word or least, so a hopeless one is wider than avail.
pub fn table_fit(natural: &[f64], minimum: &[f64], avail: f64, least: f64) -> Vec<f64> {
    let held_at = |c: usize| {
        if natural[c] < least {
            natural[c]
        } else {
            least.max(minimum[c].min(natural[c]))
        }
    };
    let capped = |w: f64, c: usize| least.min(natural[c]).max(w.floor());
    let total: f64 = natural.iter().sum();
    let floor: f64 = (0..natural.len()).map(held_at).sum();
    if total <= avail {
        return natural.to_vec();
    }
    let widths: Vec<f64> = if floor <= avail {
        (0..natural.len())
            .map(|c| {
                let held = held_at(c);
                held + (avail - floor) * (natural[c] - held) / (total - floor)
            })
            .collect()
    } else {
        // Even the longest words do not fit: every column holds its longest word whole and the table scrolls sideways.
        (0..natural.len()).map(held_at).collect()
    };
    widths
        .into_iter()
        .enumerate()
        .map(|(c, w)| capped(w, c))
        .collect()
}

/// A picture of w by h in a line limit wide: a wider one scales to the limit with its ratio kept, at least 1 tall; one that fits keeps its size.
pub fn picture_fit(w: f32, h: f32, limit: f32) -> (f32, f32) {
    if limit > 0.0 && w > limit {
        (limit, (h * limit / w).round().max(1.0))
    } else {
        (w, h)
    }
}

fn attr(root: &str, name: &str) -> Option<String> {
    let key = format!("{name}=\"");
    let mut from = 0;
    while let Some(at) = root[from..].find(&key).map(|at| from + at) {
        from = at + key.len();
        if at > 0 && super::markdown_text::is_space(root[..at].chars().next_back().unwrap_or('x')) {
            let end = root[from..].find('"')? + from;
            return Some(root[from..end].to_string());
        }
    }
    None
}

// The leading number parseFloat reads, NaN for none.
fn parse_float(text: &str) -> f64 {
    let text = text.trim_start();
    let mut end = 0;
    let bytes = text.as_bytes();
    let mut seen_dot = false;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        end = 1;
    }
    while end < bytes.len() && (bytes[end].is_ascii_digit() || (bytes[end] == b'.' && !seen_dot)) {
        seen_dot |= bytes[end] == b'.';
        end += 1;
    }
    text[..end].parse().unwrap_or(f64::NAN)
}

// The root tag of an SVG with its width, height and view box, None when it names no usable size.
fn sized_root(svg: &str) -> Option<(f64, f64, Vec<f64>)> {
    let start = svg.match_indices("<svg").map(|(at, _)| at).find(|at| {
        !svg.as_bytes()
            .get(at + 4)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    })?;
    let end = svg[start..].find('>')? + start + 1;
    let root = &svg[start..end];
    let width = parse_float(&attr(root, "width").unwrap_or_default());
    let height = parse_float(&attr(root, "height").unwrap_or_default());
    let box_text = attr(root, "viewBox").unwrap_or_default();
    let view: Vec<f64> = box_text
        .split_whitespace()
        .map(|n| n.parse().unwrap_or(f64::NAN))
        .collect();
    let usable = width > 0.0
        && height > 0.0
        && view.len() == 4
        && view.iter().all(|n| !n.is_nan())
        && view[3] > 0.0;
    usable.then_some((width, height, view))
}

/// An inline formula's box from its root: width="30px" height="20px" viewBox="0 -15 60 20" is 30 wide, 15 above the baseline and 5 below.
pub fn formula_box(svg: &str) -> Option<(f64, f64, f64)> {
    let (width, height, view) = sized_root(svg)?;
    let unit = height / view[3];
    Some((width, -view[1] * unit, (view[1] + view[3]) * unit))
}

#[cfg(test)]
#[path = "markdown_fit_tests.rs"]
mod tests;
