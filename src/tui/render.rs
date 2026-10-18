use super::{cells::{self, RowStyle}, icons, layout, keymap::Map, model::Model, theme::Theme};
use std::io::{self, Write};
pub use super::cells::{clean, safe, text_width, fit, Wrapped};
pub use super::layout::{panes, BODY_ROW, CHROME_ROWS, body_height, pane_padding, preview_area, tab_layout};
use super::layout::footer;
extern "C" { fn ctime_r(time: *const i64, buffer: *mut std::ffi::c_char) -> *mut std::ffi::c_char; }

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

// "1 row hidden by the filter", the body note the listing draws under a filtered view.
#[cfg(test)]
pub fn hidden_note(hidden: usize) -> String {
    format!("{} {} hidden by the filter", grouped(hidden), word(hidden, "row", "rows"))
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
pub fn modified(mtime: i64) -> String {
    if mtime <= 0 {
        return String::new();
    }
    const SECONDS_PER_MINUTE: u64 = 60;
    const SECONDS_PER_HOUR: u64 = 3600;
    const SECONDS_PER_DAY: u64 = 86400;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let age = now.saturating_sub(mtime as u64);
    if age < SECONDS_PER_HOUR {
        return format!("{} min", age / SECONDS_PER_MINUTE);
    }
    if age < SECONDS_PER_DAY {
        return format!("{} h", age / SECONDS_PER_HOUR);
    }
    if age < SECONDS_PER_DAY * 7 {
        return format!("{} d", age / SECONDS_PER_DAY);
    }
    let mut buffer = [0; 32];
    if unsafe { ctime_r(&mtime, buffer.as_mut_ptr()) }.is_null() {
        return String::new();
    }
    // Sample input: Mon Sep  7 23:16:02 2026; ctime_r supplies the local calendar without a subprocess.
    let stamp = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy();
    let parts: Vec<&str> = stamp.split_whitespace().collect();
    if parts.len() >= 3 {
        format!("{} {}", parts[1], parts[2])
    } else {
        String::new()
    }
}
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
                out.push_str(&cells::row(entry, middle, Some(&m.row_size(index, entry)), m.marks(), style, theme, &m.filter));
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
                else if editor.kind == "filter" { format!("  {} of {} shown", m.shown().len(), m.total) }
                else if editor.kind == "search" { format!(" · in {} · Tab changes scope", if m.search_here { layout::display_path(m) } else { "Home".into() }) }
                else { String::new() };
            out.push_str(&editor.line(&prefix, &suffix, header_width, &base, &theme.muted, &theme.accent));
        }
    } else { out.push_str(&footer(m, theme, header_width, elapsed)); }
    out.push_str(&format!("{} ", base));
    if m.sheet {
        let rows = panel_rows(m, map);
        overlay(&mut out, &rows[m.sheet_top.min(rows.len())..], columns, lines, &base, None, if m.properties.is_some() { "properties" } else { "keys" }, None);
    }
    if m.menu {
        let rows = menu_rows(m);
        let disabled: Vec<usize> = (0..rows.len()).filter(|index| !m.menu_enabled(m.menu_top + index)).collect();
        overlay(&mut out, &rows, columns, lines, &base, Some(m.menu_cursor.saturating_sub(m.menu_top)),
            if m.taildrop.submenu { "taildrop" } else { "open" }, Some((&theme.muted, &disabled)));
    }
    if let Some(deletion) = &m.deletion {
        let rows = deletion_rows(m);
        let scroll = deletion.scroll.min(rows.len().saturating_sub(1));
        overlay(&mut out, &rows[scroll..], columns, lines, &base, None, "Delete permanently?", None);
        for (destructive, x, y) in deletion_buttons(m) {
            let color = if destructive { if deletion.token == 0 { &theme.muted } else { &theme.error } } else { &theme.accent };
            out.push_str(&format!("\x1b[{};{}H{}{}{}{}{}", y, x, base, color,
                if deletion.destructive == destructive { "\x1b[7m" } else { "" }, if destructive { "[ Delete ]" } else { "[ Cancel ]" }, base));
        }
    }
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
pub fn deletion_rows(m: &Model) -> Vec<String> {
    let Some(deletion) = &m.deletion else { return Vec::new(); };
    let width = m.columns.saturating_sub(if m.columns < 30 { 4 } else { 8 }).max(1);
    let summary = if deletion.token == 0 { "Inspecting selected items…".into() }
        else { format!("{} {}, {}", grouped(deletion.count), if deletion.count == 1 { "item" } else { "items" }, bytes(deletion.bytes)) };
    let mut rows: Vec<String> = Wrapped::new(&summary, width).map(str::to_owned).collect();
    // One item is deleted as "it", several as "them", the same rule the window's confirm keeps.
    let scope = if deletion.count == 1 { "This deletes it from disk. This cannot be undone." }
        else { "This deletes them from disk. This cannot be undone." };
    rows.extend(Wrapped::new(scope, width).map(str::to_owned));
    rows.push(String::new());
    if m.columns < 30 { rows.extend([" ".repeat(10), " ".repeat(10)]); }
    else { rows.push(" ".repeat(22)); }
    rows
}
pub fn deletion_buttons(m: &Model) -> Vec<(bool, usize, usize)> {
    let rows = deletion_rows(m);
    let Some(deletion) = &m.deletion else { return Vec::new(); };
    let scroll = deletion.scroll.min(rows.len().saturating_sub(1));
    let (x, y, width, count) = overlay_rect(&rows[scroll..], m.columns, m.height + CHROME_ROWS);
    let stacked = m.columns < 30;
    [false, true].into_iter().filter_map(|destructive| {
        let row = rows.len() - 1 - usize::from(stacked && !destructive);
        if row < scroll || row >= scroll + count || width < 10 { return None; }
        let offset = if stacked { width - 10 } else { width.saturating_sub(22) + if destructive { 12 } else { 0 } };
        Some((destructive, x + 2 + offset, y + 2 + row - scroll))
    }).collect()
}
pub fn menu_rows(m: &Model) -> Vec<String> {
    if m.taildrop.submenu {
        return m.taildrop.peers.iter().skip(m.menu_top).map(|p| p.label.clone()).collect();
    }
    let reason = m.taildrop.reason();
    let taildrop = if reason.is_empty() { "taildrop  ▶".into() } else { format!("taildrop · {}", reason) };
    vec!["open".into(), if m.hidden { "hide hidden".into() } else { "show hidden".into() }, taildrop]
        .into_iter().skip(m.menu_top).collect()
}
pub fn panel_rows(m: &Model, map: &Map) -> Vec<String> {
    let rows = m.properties.clone().unwrap_or_else(|| map.sheet(&m.preset));
    rows.iter().flat_map(|line| Wrapped::new(line, m.columns.saturating_sub(8).max(1))).map(clean).collect()
}
pub fn overlay_rect(rows: &[String], columns: usize, lines: usize) -> (usize, usize, usize, usize) {
    let width = rows
        .iter()
        .map(|row| text_width(row))
        .max()
        .unwrap_or(0)
        .saturating_add(2)
        .max(13)
        .min(columns.saturating_sub(if columns < 30 { 2 } else { 6 }));
    let count = rows.len().min(lines.saturating_sub(4));
    (
        (columns.saturating_sub(width + 2)) / 2,
        (lines.saturating_sub(count + 2)) / 2,
        width,
        count,
    )
}
fn overlay(
    out: &mut String,
    rows: &[String],
    columns: usize,
    lines: usize,
    base: &str,
    selected: Option<usize>,
    title: &str,
    dim: Option<(&str, &[usize])>,
) {
    let (x, y, width, count) = overlay_rect(rows, columns, lines);
    let heading = format!("─ {} ", title);
    out.push_str(&format!(
        "\x1b[{};{}H{}┌{}{}┐",
        y + 1,
        x + 1,
        base,
        fit(&heading, text_width(&heading).min(width)),
        "─".repeat(width.saturating_sub(text_width(&heading)))
    ));
    for (i, row) in rows.iter().take(count).enumerate() {
        out.push_str(&format!(
            "\x1b[{};{}H{}│{}{} {} {}│",
            y + i + 2,
            x + 1,
            base,
            dim.filter(|(_, indices)| indices.contains(&i))
                .map(|(color, _)| color)
                .unwrap_or(""),
            if selected == Some(i) { "\x1b[7m" } else { "" },
            fit(row, width.saturating_sub(2)),
            base
        ));
    }
    out.push_str(&format!(
        "\x1b[{};{}H{}└{}┘",
        y + count + 2,
        x + 1,
        base,
        "─".repeat(width)
    ));
}
#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "frame_tests.rs"]
pub(crate) mod frame_tests;
