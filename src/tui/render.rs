use super::{
    keymap::Map,
    model::{Model, Row},
    theme::Theme,
};
use std::io::{self, Write};

extern "C" {
    fn wcwidth(c: i32) -> i32;
    fn ctime_r(time: *const i64, buffer: *mut std::ffi::c_char) -> *mut std::ffi::c_char;
}
pub fn clean(text: &str) -> String {
    text.chars().filter(|c| safe(*c)).collect()
}
pub fn safe(c: char) -> bool {
    !c.is_control()
        && !matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{200e}'|'\u{200f}'|'\u{061c}')
}
fn width(c: char) -> usize {
    let n = unsafe { wcwidth(c as i32) };
    if n < 0 {
        1
    } else {
        n as usize
    }
}
pub fn text_width(text: &str) -> usize {
    clean(text).chars().map(width).sum()
}
pub struct Wrapped<'a> {
    remaining: Option<&'a str>,
    columns: usize,
}
impl<'a> Wrapped<'a> {
    pub fn new(text: &'a str, columns: usize) -> Self {
        Self {
            remaining: (columns > 0).then_some(text),
            columns,
        }
    }
}
impl<'a> Iterator for Wrapped<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        let text = self.remaining.take()?;
        let mut used = 0;
        for (offset, c) in text.char_indices() {
            let cells = width(c);
            if used + cells > self.columns && offset > 0 {
                self.remaining = Some(&text[offset..]);
                return Some(&text[..offset]);
            }
            used += cells;
        }
        Some(text)
    }
}
pub fn panes(columns: usize, preview: bool) -> (usize, usize, usize) {
    // Tui.html uses border-box widths: each separator belongs to the pane on its left.
    let left = (columns * 22 / 100).saturating_sub(1);
    let middle = if preview {
        (columns * 40 / 100).saturating_sub(1)
    } else {
        columns.saturating_sub(left + 1)
    };
    (
        left,
        middle,
        if preview {
            columns.saturating_sub(left + middle + 2)
        } else {
            0
        },
    )
}
pub const BODY_ROW: usize = 3;
pub const CHROME_ROWS: usize = 4;
pub fn body_height(lines: usize) -> usize {
    lines.saturating_sub(CHROME_ROWS)
}
pub fn pane_padding(columns: usize) -> usize {
    const MIN_CONTENT: usize = 5; // A focused PDF control and both overflow arrows must remain visible.
    (columns.saturating_sub(MIN_CONTENT) / 2).min(1)
}
pub fn preview_area(columns: usize, quicklook: bool) -> (usize, usize) {
    let (left, middle, right) = panes(columns, true);
    let (start, width) = if quicklook { (0, columns) } else { (left + middle + 2, right) };
    let padding = pane_padding(width);
    (start + padding, width - padding * 2)
}
pub fn fit(text: &str, limit: usize) -> String {
    let mut result = String::new();
    let mut used = 0;
    for c in clean(text).chars() {
        let n = width(c);
        if used + n > limit {
            break;
        }
        result.push(c);
        used += n;
    }
    result.push_str(&" ".repeat(limit - used));
    result
}
fn display_path(m: &Model) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() && m.path.starts_with(&home) {
        format!("~{}", &m.path.to_string_lossy()[home.len()..])
    } else { m.path.to_string_lossy().into_owned() }
}
fn header_title(m: &Model) -> String {
    format!("{} · {} {}", display_path(m), m.sort, if m.reverse { "▾" } else { "▴" })
}
pub fn tab_layout(m: &Model, columns: usize) -> Vec<(usize, String)> {
    let available = columns.saturating_sub(text_width(&header_title(m)).min(columns / 2));
    let label = |index: usize| format!("{} {}  ", index + 1, m.tabs[index].path.file_name().unwrap_or_default().to_string_lossy());
    let mut first = m.tab;
    let mut used = text_width(&label(m.tab)).min(available);
    while first > 0 {
        let width = text_width(&label(first - 1));
        if used + width > available { break; }
        used += width;
        first -= 1;
    }
    used = 0;
    let mut tabs = Vec::new();
    for index in first..m.tabs.len() {
        let text = label(index);
        let width = text_width(&text).min(available.saturating_sub(used));
        if width == 0 { break; }
        tabs.push((index, fit(&text, width)));
        used += width;
    }
    tabs
}
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
fn match_range(text: &str, query: &str) -> Option<(usize, usize)> {
    if query.is_empty() { return None; }
    let query = query.to_lowercase();
    let start = text.to_lowercase().find(&query)?;
    let end = start + query.len();
    let mut offset = 0;
    let mut first = None;
    for (index, c) in text.char_indices() {
        let length: usize = c.to_lowercase().map(char::len_utf8).sum();
        if offset <= start && start < offset + length { first = Some(index); }
        if offset < end && end <= offset + length { return first.map(|first| (first, index + c.len_utf8())); }
        offset += length;
    }
    None
}
fn mark(row: &Row) -> &'static str {
    if !row.link.is_empty() {
        "@"
    } else if row.directory {
        "›"
    } else if row.mode & 0o111 != 0 {
        "*"
    } else {
        "□"
    }
}
fn row(row: &Row, columns: usize) -> (String, String) {
    let label = format!(
        "{} {}{}{}",
        mark(row),
        row.name,
        if row.directory { "/" } else { "" },
        if row.link.is_empty() {
            String::new()
        } else {
            format!(" → {}", row.link)
        }
    );
    if columns > 18 && row.link.is_empty() {
        let size = if row.directory {
            modified(row.modified)
        } else {
            bytes(row.size)
        };
        (fit(&label, columns - size.len() - 1), format!(" {}", size))
    } else {
        (fit(&label, columns), String::new())
    }
}
pub fn draw(
    m: &Model,
    theme: &Theme,
    map: &Map,
    columns: usize,
    lines: usize,
    cell_pixels: Option<(usize, usize)>,
    elapsed: std::time::Duration,
    last: &mut String,
) -> io::Result<bool> {
    if columns < 12 || lines <= CHROME_ROWS {
        print!("\x1b[H\x1b[2J{}", fit("Window too small", columns));
        io::stdout().flush()?;
        return Ok(true);
    }
    let (left, middle, right) = panes(columns, m.preview_visible);
    let left_padding = pane_padding(left);
    let middle_padding = pane_padding(middle);
    let preview_padding = pane_padding(if m.quicklook { columns } else { right });
    let (_, preview_width) = preview_area(columns, m.quicklook);
    let filtered = (!m.filter.is_empty()).then(|| m.shown());
    let body = body_height(lines);
    let graphical = m.player.is_some() || m.pdf.is_some() || m.image_file.is_some();
    let details = m.preview_metadata.len().min(3);
    let details_start = body.saturating_sub(2 + details + usize::from(m.pdf.is_some()));
    let base = format!("\x1b[0m{}{}", theme.background, theme.foreground);
    let mut out = format!("\x1b[H{}", base);
    let path = display_path(m);
    let title = header_title(m);
    let header_width = columns.saturating_sub(2);
    let title_width = text_width(&title).min(header_width / 2);
    let mut used = 0;
    out.push(' ');
    for (i, label) in tab_layout(m, header_width) {
        let width = text_width(&label);
        out.push_str(if i == m.tab {
            &theme.accent
        } else {
            &theme.foreground
        });
        out.push_str(&clean(&label));
        used += width;
    }
    out.push_str(&base);
    out.push_str(&" ".repeat(header_width.saturating_sub(used + title_width)));
    out.push_str(&fit(&title, title_width));
    out.push(' ');
    out.push_str(&format!("\x1b[2;1H{}{}{}", theme.border, "─".repeat(columns), base));
    for y in 0..body {
        out.push_str(&format!("\x1b[{};1H{}", y + BODY_ROW, base));
        if m.quicklook {
            let content = if m.selected.len() > 1 {
                selection_line(m, y, preview_width)
            } else if y == body.saturating_sub(2) {
                m.player
                    .as_ref()
                    .map(|p| p.line(preview_width))
                    .or_else(|| m.pdf.as_ref().map(|p| p.line(true, preview_width)))
                    .unwrap_or_default()
            } else if m.pdf.is_some() && y == body.saturating_sub(3) {
                m.pdf.as_ref().unwrap().prefix()
            } else if graphical && y >= details_start && y < details_start + details {
                m.preview_metadata[y - details_start].clone()
            } else if graphical && y >= 2 {
                String::new()
            } else {
                m.preview_display.get(y).cloned().unwrap_or_default()
            };
            out.push_str(&" ".repeat(preview_padding));
            out.push_str(&fit(&content, preview_width));
            out.push_str(&" ".repeat(preview_padding));
            continue;
        }
        if let Some(parent) = m.parents.get(y) {
            if m.path
                .file_name()
                .is_some_and(|name| name == parent.name.as_str())
            {
                out.push_str(&theme.selected);
            }
            let label = format!(
                "{} {}",
                if parent.directory { "›" } else { "□" },
                parent.name
            );
            out.push_str(&" ".repeat(left_padding));
            out.push_str(&fit(&label, left - left_padding * 2));
            out.push_str(&" ".repeat(left_padding));
        } else {
            out.push_str(&" ".repeat(left));
        }
        out.push_str(&base);
        out.push_str(&theme.border);
        out.push('│');
        out.push_str(&base);
        let index = filtered.as_ref().map_or(m.top + y, |rows| rows.get(y).copied().unwrap_or(usize::MAX));
        if let Some(entry) = m.rows.get(&index) {
            let row_color = if index == m.cursor {
                format!("{}\x1b[7m", theme.accent)
            } else if m.selected.contains(&index) {
                format!("{}{}", theme.foreground, theme.selected)
            } else if !entry.link.is_empty() {
                theme.symlink.clone()
            } else if entry.mode & 0o111 != 0 && !entry.directory {
                theme.executable.clone()
            } else { theme.foreground.clone() };
            out.push_str(&row_color);
            out.push_str(&" ".repeat(middle_padding));
            let content_width = middle - middle_padding * 2;
            if let Some(editor) = m.editor.as_ref().filter(|e| e.kind == "rename" && e.path == m.row_path(entry)) {
                out.push_str(&base);
                out.push_str(&editor.line(&format!("{} ", mark(entry)), "", content_width, &base, &theme.muted, &theme.accent));
            } else {
                let (label, metadata) = row(entry, content_width);
                if let Some((start, end)) = (index != m.cursor).then(|| match_range(&label, &m.filter)).flatten() {
                    out.push_str(&label[..start]);
                    out.push_str(&theme.accent);
                    out.push_str(&label[start..end]);
                    out.push_str(&row_color);
                    out.push_str(&label[end..]);
                } else { out.push_str(&label); }
                if index != m.cursor { out.push_str(&theme.foreground); }
                out.push_str(&metadata);
            }
            out.push_str(&" ".repeat(middle_padding));
            out.push_str(&base);
        } else if m.total == 0 {
            if m.pending.is_some() {
                out.push_str(&fit(if y == body / 2 { "Loading…" } else { "" }, middle));
            } else {
                out.push_str(&super::empty::line(y, body, middle, cell_pixels, elapsed));
            }
        } else if filtered.as_ref().is_some_and(|rows| y == rows.len()) {
            let count = filtered.as_ref().unwrap().len();
            let note = if count == 0 { format!("Nothing matches {}", m.filter) }
                else { format!("{} rows hidden by the filter", m.rows.len() - count) };
            out.push_str(&" ".repeat(middle_padding));
            out.push_str(&fit(&note, middle - middle_padding * 2));
            out.push_str(&" ".repeat(middle_padding));
        } else {
            out.push_str(&" ".repeat(middle));
        }
        if !m.preview_visible {
            continue;
        }
        out.push_str(&theme.border);
        out.push('│');
        out.push_str(&base);
        let preview = if y == body.saturating_sub(2) && (m.player.is_some() || m.pdf.is_some()) {
            m.player
                .as_ref()
                .map(|p| p.line(preview_width))
                .or_else(|| m.pdf.as_ref().map(|p| p.line(false, preview_width)))
                .unwrap_or_default()
        } else if m.pdf.is_some() && y == body.saturating_sub(3) {
            m.pdf.as_ref().unwrap().prefix()
        } else if m.selected.len() > 1 {
            selection_line(m, y, preview_width)
        } else if graphical && y >= details_start && y < details_start + details {
            m.preview_metadata[y - details_start].clone()
        } else if graphical && y >= 2 {
            String::new()
        } else if m.preview_visible || m.quicklook {
            m.preview_display.get(y).cloned().unwrap_or_default()
        } else {
            String::new()
        };
        out.push_str(if m.selected.len() > 1 && y == 0 {
            &theme.accent
        } else {
            &theme.foreground
        });
        out.push_str(&" ".repeat(preview_padding));
        out.push_str(&fit(&preview, preview_width));
        out.push_str(&" ".repeat(preview_padding));
        out.push_str(&base);
    }
    out.push_str(&format!("\x1b[{};1H{}{}{}", lines - 1, theme.border, "─".repeat(columns), base));
    out.push_str(&format!("\x1b[{};1H{} ", lines, base));
    if let Some(editor) = &m.editor {
        if editor.kind == "rename" {
            let notice = if !editor.error.is_empty() { editor.error.as_str() }
                else if editor.pending { "Renaming…" }
                else { "Enter saves · Escape cancels · Ctrl+A selects the full name" };
            if !editor.error.is_empty() { out.push_str(&theme.error); }
            out.push_str(&fit(notice, header_width));
        } else {
        let prefix = if editor.kind == "path" {
            ": ".into()
        } else {
            format!("{}: ", editor.kind)
        };
        let suffix = if editor.pending {
            " · Working…".into()
        } else if editor.kind == "path" {
            m.completion
                .strip_prefix(&editor.value)
                .unwrap_or("")
                .into()
        } else if editor.kind == "search" {
            format!(
                " · in {} · Tab changes scope",
                if m.search_here {
                    path.clone()
                } else {
                    "Home".into()
                }
            )
        } else {
            String::new()
        };
        out.push_str(&editor.line(&prefix, &suffix, header_width, &base, &theme.muted, &theme.accent));
        }
    } else {
        out.push_str(&footer(m, theme, header_width, elapsed));
    }
    out.push(' ');
    if m.sheet {
        let sheet = panel_rows(m, map);
        overlay(
            &mut out,
            &sheet[m.sheet_top.min(sheet.len())..],
            columns,
            lines,
            &base,
            None,
            if m.properties.is_some() { "properties" } else { "keys" },
            None,
        );
    }
    if m.menu {
        let rows = menu_rows(m);
        let disabled: Vec<usize> = (0..rows.len()).filter(|index| !m.menu_enabled(m.menu_top + index)).collect();
        overlay(
            &mut out,
            &rows,
            columns,
            lines,
            &base,
            Some(m.menu_cursor.saturating_sub(m.menu_top)),
            if m.taildrop.submenu {
                "taildrop"
            } else {
                "open"
            },
            Some((&theme.muted, &disabled)),
        );
    }
    if let Some(deletion) = &m.deletion {
        let rows = deletion_rows(m);
        let scroll = deletion.scroll.min(rows.len().saturating_sub(1));
        let shown = &rows[scroll..];
        overlay(&mut out, shown, columns, lines, &base, None, "Delete permanently?", None);
        for (destructive, x, y) in deletion_buttons(m) {
            let color = if destructive { if deletion.token == 0 { &theme.muted } else { &theme.error } } else { &theme.accent };
            out.push_str(&format!("\x1b[{};{}H{}{}{}{}{}", y, x, base, color,
                if deletion.destructive == destructive { "\x1b[7m" } else { "" },
                if destructive { "[ Delete ]" } else { "[ Cancel ]" }, base));
        }
    }
    if *last != out {
        print!("{}\x1b[0m", out);
        io::stdout().flush()?;
        *last = out;
        return Ok(true);
    }
    Ok(false)
}
pub fn deletion_rows(m: &Model) -> Vec<String> {
    let Some(deletion) = &m.deletion else { return Vec::new(); };
    let width = m.columns.saturating_sub(if m.columns < 30 { 4 } else { 8 }).max(1);
    let summary = if deletion.token == 0 { "Inspecting selected items…".into() }
        else { format!("{} {}, {}", deletion.count, if deletion.count == 1 { "item" } else { "items" }, bytes(deletion.bytes)) };
    let mut rows: Vec<String> = Wrapped::new(&summary, width).map(str::to_owned).collect();
    rows.extend(Wrapped::new("This deletes them from disk. This cannot be undone.", width).map(str::to_owned));
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
fn footer(m: &Model, theme: &Theme, columns: usize, elapsed: std::time::Duration) -> String {
    let base = format!("\x1b[0m{}{}", theme.background, theme.foreground);
    let help = "? keys";
    let available = columns.saturating_sub(text_width(help) + 2);
    let mut parts = Vec::new();
    let chip = format!("{}\x1b[7m", theme.accent);
    if !m.selected.is_empty() { parts.push((chip.as_str(), format!(" V {} ", m.selected.len()))); }
    parts.push((theme.foreground.as_str(), format!("{} items", m.total)));
    if !m.error.is_empty() {
        let queued = if m.errors.is_empty() { String::new() } else { format!(" (+{})", m.errors.len()) };
        parts.push((theme.error.as_str(), format!("{}{} · Esc dismisses", m.error, queued)));
    }
    if !m.transfer.is_empty() {
        const FRAMES: [&str; 3] = ["░▒▓", "▒▓░", "▓░▒"];
        const FRAME_MILLIS: u128 = 200;
        parts.push((theme.foreground.as_str(), format!("{} {}", m.transfer, FRAMES[(elapsed.as_millis() / FRAME_MILLIS) as usize % FRAMES.len()])));
    }
    if !m.search.is_empty() { parts.push((theme.foreground.as_str(), m.search.clone())); }
    if m.error.is_empty() && m.transfer.is_empty() && m.search.is_empty() && !m.message.is_empty() {
        parts.push((theme.foreground.as_str(), m.message.clone()));
    }
    if !m.filter.is_empty() {
        parts.push((theme.foreground.as_str(), format!("Filter {} · {} matches in {} loaded rows", m.filter, m.shown().len(), m.rows.len())));
    }
    let mut out = String::new();
    let mut used = 0;
    for (color, text) in parts {
        if used >= available { break; }
        if used > 0 {
            let gap = 2.min(available - used);
            out.push_str(&" ".repeat(gap));
            used += gap;
        }
        let width = text_width(&text).min(available - used);
        out.push_str(color);
        out.push_str(&fit(&text, width));
        out.push_str(&base);
        used += width;
    }
    out.push_str(&" ".repeat(columns.saturating_sub(used + text_width(help))));
    out.push_str(help);
    out
}
fn selection_line(m: &Model, y: usize, columns: usize) -> String {
    if y == 0 {
        return format!("{} items selected", m.selected.len());
    }
    if y == 1 {
        return "─".repeat(columns);
    }
    if let Some(row) = m.selected_rows.values().nth(y - 2) {
        let size = bytes(row.size);
        return format!(
            "{} {}",
            fit(&row.name, columns.saturating_sub(size.len() + 1)),
            size
        );
    }
    let footer = y.saturating_sub(m.selected_rows.len() + 2);
    if footer == 1 {
        if m.selected_rows.len() == m.selected.len() {
            let total = m
                .selected_rows
                .values()
                .fold(0usize, |sum, row| sum.saturating_add(row.size));
            return format!("Selection total · {}", bytes(total));
        }
        return format!(
            "{} marked items outside loaded rows",
            m.selected.len() - m.selected_rows.len()
        );
    }
    if footer >= 2 {
        return Wrapped::new("Preview follows the marked set while visual mode is active.", columns).nth(footer - 2).unwrap_or("").into();
    }
    String::new()
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
mod tests {
    use super::*;
    #[test]
    fn chrome_and_pane_insets_preserve_body_and_minimum_pdf_space() {
        assert_eq!(body_height(67), 63);
        assert_eq!(BODY_ROW + body_height(67), 66);
        assert_eq!(preview_area(277, false), (171, 105));
        assert_eq!(preview_area(277, true), (1, 275));
        for columns in 12..=277 {
            for quicklook in [false, true] {
                let (start, width) = preview_area(columns, quicklook);
                assert!(width >= 5 && start + width <= columns);
                let outer = if quicklook { columns } else { panes(columns, true).2 };
                assert_eq!(start + width + pane_padding(outer), columns);
            }
        }
    }
    #[test]
    fn scrolled_root_menu_renders_the_selected_action() {
        let mut model = Model::new(std::path::PathBuf::from("/"), &crate::jsondoc::Json::Null);
        model.columns = 80;
        model.height = 4;
        model.menu_top = 1;
        model.menu_cursor = 2;
        let rows = menu_rows(&model);
        assert_eq!(rows, ["show hidden", "taildrop  ▶"]);
        assert_eq!(overlay_rect(&rows, model.columns, model.height + CHROME_ROWS).3, 2);
        let mut output = String::new();
        overlay(&mut output, &rows, model.columns, model.height + CHROME_ROWS, "",
            Some(model.menu_cursor - model.menu_top), "open", None);
        assert!(output.contains("\x1b[7m taildrop  ▶ │"));
        assert!(!output.contains("\x1b[7m show hidden"));
        assert!(output.contains("│ show hidden │"));
        model.hidden = true;
        assert_eq!(menu_rows(&model), ["hide hidden", "taildrop  ▶"]);
        model.taildrop.error = "signed out".into();
        assert_eq!(menu_rows(&model), ["hide hidden", "taildrop · signed out"]);
    }
    #[test]
    fn confirmation_buttons_stay_visible_and_hit_testable_at_small_sizes() {
        let mut model = Model::new(std::path::PathBuf::from("/"), &crate::jsondoc::Json::Null);
        model.deletion = Some(super::super::model::Deletion { token: 1, count: 3, bytes: 42, ..Default::default() });
        assert!(!model.deletion.as_ref().unwrap().destructive);
        for columns in [12, 28, 80] {
            for height in [4, 20] {
                model.columns = columns;
                model.height = height;
                let scroll = deletion_rows(&model).len().saturating_sub(height);
                model.deletion.as_mut().unwrap().scroll = scroll;
                let buttons = deletion_buttons(&model);
                assert_eq!(buttons.len(), 2);
                for (_, x, y) in &buttons {
                    assert!(*x > 0 && x + 9 <= columns);
                    assert!(*y > 0 && *y <= height + CHROME_ROWS);
                }
                assert_ne!((buttons[0].1, buttons[0].2), (buttons[1].1, buttons[1].2));
            }
        }
    }
    #[test]
    fn padded_overlays_preserve_confirmation_and_panel_text() {
        let mut model = Model::new(std::path::PathBuf::from("/"), &crate::jsondoc::Json::Null);
        model.deletion = Some(super::super::model::Deletion { token: 1, count: 3, bytes: 42, ..Default::default() });
        let map = Map::load();
        for columns in [12, 28, 30, 80] {
            model.columns = columns;
            let confirmation = deletion_rows(&model);
            assert!(confirmation.concat().contains("This deletes them from disk. This cannot be undone."));
            model.properties = None;
            let keys = panel_rows(&model, &map);
            model.properties = Some(vec!["Properties: a-long-file-name-with-an-extension.txt".into()]);
            let properties = panel_rows(&model, &map);
            for rows in [confirmation, keys, properties] {
                let (_, _, width, _) = overlay_rect(&rows, columns, rows.len() + 4);
                let mut output = String::new();
                overlay(&mut output, &rows, columns, rows.len() + 4, "", None, "panel", None);
                for row in rows.iter().filter(|row| !row.trim().is_empty()) {
                    assert!(text_width(row) <= width - 2, "clipped {row:?} at {columns} columns");
                    assert!(output.contains(&format!("│ {} │", fit(row, width - 2))));
                }
            }
        }
    }
    #[test]
    fn active_tab_remains_in_the_same_visible_hit_map() {
        let mut model = Model::new(std::path::PathBuf::from("/tab-one"), &crate::jsondoc::Json::Null);
        for index in 2..=12 {
            model.tabs.push(super::super::model::Tab { path: format!("/tab-{index}").into(), cursor: 0, back: Vec::new(), forward: Vec::new() });
        }
        model.tab = 11;
        for columns in [12, 40, 80, 200] {
            let tabs = tab_layout(&model, columns);
            assert!(tabs.iter().any(|(index, _)| *index == model.tab));
            assert!(tabs.iter().map(|(_, label)| text_width(label)).sum::<usize>() <= columns);
        }
    }
    #[test]
    fn footer_separates_error_color_and_preserves_secondary_progress() {
        let mut model = Model::new(std::path::PathBuf::from("/"), &crate::jsondoc::Json::Null);
        model.fail("Denied".into());
        model.fail("Missing".into());
        model.transfer = "Copying 2 of 5".into();
        model.search = "Searching 12 matches".into();
        let theme = Theme::from_text("");
        let output = footer(&model, &theme, 120, std::time::Duration::ZERO);
        assert!(output.contains(&format!("{}Denied (+1) · Esc dismisses\x1b[0m", theme.error)));
        assert!(output.find("Denied").unwrap() < output.find("Copying").unwrap());
        assert!(output.find("Copying").unwrap() < output.find("Searching").unwrap());
        assert!(output.ends_with("? keys"));
        assert_eq!(output.matches(&theme.error).count(), 1);
    }
    #[test]
    fn unsafe_names_cannot_emit_terminal_commands() {
        assert_eq!(clean("a\x1b]52;c;evil\x07\u{202e}b"), "a]52;c;evilb");
        assert_eq!(fit("abcdef", 3), "abc");
        assert_eq!(fit("a", 3), "a  ");
        assert_eq!(
            Wrapped::new("abcdef", 2).collect::<Vec<_>>(),
            vec!["ab", "cd", "ef"]
        );
        assert_eq!(Wrapped::new("", 2).collect::<Vec<_>>(), vec![""]);
        assert!(Wrapped::new("abc", 0).next().is_none());
        assert_eq!(panes(100, true), (21, 39, 38));
        assert_eq!(panes(100, false), (21, 78, 0));
        assert_eq!(bytes(999), "999 B");
        assert_eq!(bytes(1000), "1.0 kB");
        assert_eq!(bytes(1_200_000_000), "1.2 GB");
        let text = "□ İ.txt";
        let (start, end) = match_range(text, "i").unwrap();
        assert_eq!(&text[start..end], "İ");
    }
}
