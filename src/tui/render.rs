use super::{cells::{self, RowStyle}, icons, layout, keymap::Map, model::Model, theme::Theme};
use std::io::{self, Write};
pub use super::cells::{clean, safe, text_width, fit};
pub use super::layout::{panes, BODY_ROW, CHROME_ROWS, body_height, pane_padding, preview_area, tab_layout};
use super::layout::footer;
#[path = "cards.rs"]
pub(super) mod cards;
#[path = "key_sheet.rs"]
pub(super) mod key_sheet;
#[path = "menu_layout.rs"]
pub(super) mod menu_layout;
#[path = "menu_run.rs"]
pub(super) mod menu_run;

pub fn bytes(n: usize) -> String {
    const BYTES_PER_UNIT: f64 = 1000.0;
    const UNITS: [&str; 5] = ["B", "kB", "MB", "GB", "TB"];
    if n < BYTES_PER_UNIT as usize {
        return format!("{} B", n);
    }
    let mut value = n as f64;
    let mut unit = 0;
    while value >= BYTES_PER_UNIT && unit < UNITS.len() - 1 {
        value /= BYTES_PER_UNIT;
        unit += 1;
    }
    format!("{:.1} {}", value, UNITS[unit])
}
// The one-vs-many rule every counted line shares, so one item never reads "1 items".
pub fn word(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

// "1 item" or "1,204 items", so no caller builds a plural by hand.
pub fn items(n: usize) -> String {
    format!("{} {}", grouped(n), word(n, "item", "items"))
}

// "1 marked item" or "2 marked items", the selection count past the loaded rows.
fn marked(n: usize) -> String {
    format!("{} marked {}", grouped(n), word(n, "item", "items"))
}

// Counts of rows, items, files and matches group in thousands, the way the GUI formats them.
pub fn grouped(n: usize) -> String {
    let digits = n.to_string().into_bytes();
    let mut out = Vec::with_capacity(digits.len() + digits.len() / 3);
    for (i, &digit) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(b',');
        }
        out.push(digit);
    }
    String::from_utf8(out).unwrap_or_else(|_| n.to_string())
}
pub use super::calendar::modified;
pub fn draw(m: &Model, theme: &Theme, map: &Map, columns: usize, lines: usize,
    _cell_pixels: Option<(usize, usize)>, elapsed: std::time::Duration, last: &mut String) -> io::Result<bool> {
    let out = frame(m, theme, map, columns, lines, elapsed);
    if *last == out { return Ok(false); }
    print!("{}\x1b[0m", out);
    io::stdout().flush()?;
    *last = out;
    Ok(true)
}
pub(super) fn frame(m: &Model, theme: &Theme, map: &Map, columns: usize, lines: usize, elapsed: std::time::Duration) -> String {
    if columns < 12 || lines <= CHROME_ROWS { return layout::small(columns, lines, theme); }
    // A card or the keys sheet is the one lit surface: everything behind it draws in the scrim's inks.
    let lit = theme;
    let theme = if m.sheet || m.deletion.is_some() { theme.dim.as_deref().unwrap_or(theme) } else { theme };
    let (left, middle, right) = panes(columns, m.preview_visible);
    let body = body_height(lines);
    let base = theme.base();
    let visible = m.visible();
    let mut out = format!("\x1b[H{}\x1b[2;1H{}", layout::header(m, theme, columns), layout::rule(columns, m.preview_visible, false, theme));
    for y in 0..body {
        out.push_str(&format!("\x1b[{};1H{}", y + BODY_ROW, base));
        if m.quicklook { out.push_str(&layout::preview(m, y, columns, body, theme)); continue; }
        if let Some(parent) = m.parents.get(y) {
            let current = m.path.file_name().is_some_and(|name| name == parent.name.as_str());
            out.push_str(&cells::row(parent, left, None, m.marks(), RowStyle { cursor: current, quiet: true, parent: true, ..Default::default() }, theme, ""));
        } else { out.push_str(&" ".repeat(left)); }
        out.push_str(&format!("{}│{}", theme.border, base));
        if let Some((index, entry)) = visible.get(y).and_then(|index| m.rows.get(index).map(|entry| (*index, entry))) {
            if let Some(editor) = m.editor.as_ref().filter(|e| e.kind == "rename" && e.path == m.row_path(entry)) {
                let padding = pane_padding(middle);
                out.push_str(&" ".repeat(padding));
                out.push_str(&editor.line(&format!("{} ", icons::row(entry, m.marks())), "", middle - padding * 2, &base, &theme.muted, &theme.accent));
                out.push_str(&" ".repeat(padding));
            } else {
                let style = RowStyle { cursor: index == m.cursor, focused: layout::focused(m), marked: m.selected.contains(&index), dimmed: entry.mode == 0, ..Default::default() };
                out.push_str(&cells::row(entry, middle, Some(&super::detail::value(m, index, entry)), m.marks(), style, theme, &m.filter));
            }
        } else if m.total == 0 && m.pending.is_none() {
            out.push_str(&theme.meta);
            out.push_str(&super::empty::line(y, body, middle, m.marks(), m.empty_at.elapsed()));
        } else if !m.filter.is_empty() && visible.is_empty() {
            out.push_str(&theme.meta);
            out.push_str(&super::empty::notice(y, body, middle, if m.marks() { icons::FOLDER } else { "›" }, &format!("Nothing matches {}", m.filter)));
        } else {
            out.push_str(&fit(if m.pending.is_some() && y == body / 2 { "Loading…" } else { "" }, middle));
        }
        out.push_str(&base);
        if m.preview_visible {
            out.push_str(&format!("{}│{}", theme.border, base));
            out.push_str(&layout::preview(m, y, right, body, theme));
        }
    }
    if let Some((top, height)) = layout::thumb(m, body, std::time::Instant::now()) {
        for y in top..top + height { out.push_str(&format!("\x1b[{};{}H{}▐{}", BODY_ROW + y, left + middle + 1, theme.muted, base)); }
    }
    out.push_str(&format!("\x1b[{};1H{}\x1b[{};1H{} ", lines - 1, layout::rule(columns, m.preview_visible, true, theme), lines, base));
    let header_width = columns.saturating_sub(2);
    if let Some(editor) = &m.editor {
        if editor.kind == "rename" {
            let notice = if !editor.error.is_empty() { editor.error.as_str() } else if editor.pending { "Renaming…" }
                else { "Enter saves · Escape cancels · Ctrl+A selects the full name" };
            if !editor.error.is_empty() { out.push_str(&theme.error); }
            out.push_str(&fit(notice, header_width));
        } else {
            let prefix = match editor.kind { "path" => ": ".into(), "filter" => "Filter ".into(), _ => format!("{}: ", editor.kind) };
            let suffix = if editor.pending { " · Working…".into() }
                else if editor.kind == "path" { m.completion.strip_prefix(&editor.value).unwrap_or("").into() }
                else if editor.kind == "filter" { format!("  {} of {} shown", m.filter_count(), m.total) }
                else if editor.kind == "search" { format!(" · in {} · Tab changes scope", if m.search_here { layout::display_path(m) } else { "Home".into() }) }
                else { String::new() };
            out.push_str(&editor.line(&prefix, &suffix, header_width, &base, &theme.muted, &theme.accent));
        }
    } else { out.push_str(&footer(m, theme, header_width, elapsed)); }
    out.push_str(&format!("{} ", base));
    if m.properties.is_some() { cards::draw_properties(&mut out, m, lit); }
    else if m.sheet { key_sheet::draw(&mut out, m, lit, map); }
    if m.menu { menu_layout::draw(&mut out, m, lit, map); }
    if m.deletion.is_some() { cards::draw_delete(&mut out, m, lit); }
    out
}
pub(super) fn selection_line(m: &Model, y: usize, columns: usize) -> String {
    if y == 0 { return format!("{} selected", items(m.selected.len())); }
    if y == 1 { return layout::selection_bytes(m).map(|total| format!("Combined · {}", bytes(total))).unwrap_or_default(); }
    if y < 3 { return String::new(); }
    if let Some(row) = m.selected_rows.values().nth(y - 3) { return format!("{} {}", fit(&row.name, columns.saturating_sub(text_width(&bytes(row.size)) + 1)), bytes(row.size)); }
    if y == m.selected_rows.len() + 3 && m.selected_rows.len() < m.selected.len() { return format!("{} outside loaded rows", marked(m.selected.len() - m.selected_rows.len())); }
    String::new()
}
#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "frame_tests.rs"]
pub(crate) mod frame_tests;

#[cfg(test)]
#[path = "jobrender_tests.rs"]
mod jobrender_tests;
