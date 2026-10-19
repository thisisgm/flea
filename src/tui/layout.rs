use super::{cells::{self, ellipsis, fit, text_width}, icons, model::Model, theme::{Depth, Theme}};
use std::time::{Duration, Instant};

pub const BODY_ROW: usize = 3;
pub const CHROME_ROWS: usize = 4;
pub fn body_height(lines: usize) -> usize { lines.saturating_sub(CHROME_ROWS) }
pub fn panes(columns: usize, preview: bool) -> (usize, usize, usize) {
    let left = (columns * 22 / 100).saturating_sub(1);
    let middle = if preview { (columns * 40 / 100).saturating_sub(1) } else { columns.saturating_sub(left + 1) };
    (left, middle, if preview { columns.saturating_sub(left + middle + 2) } else { 0 })
}
pub fn pane_padding(columns: usize) -> usize { (columns.saturating_sub(5) / 2).min(1) }
pub fn preview_area(columns: usize, quicklook: bool) -> (usize, usize) {
    let (left, middle, right) = panes(columns, true);
    let (start, width) = if quicklook { (0, columns) } else { (left + middle + 2, right) };
    let padding = pane_padding(width);
    (start + padding, width - padding * 2)
}
pub fn display_path(m: &Model) -> String {
    let home = std::env::var_os("HOME").filter(|s| !s.is_empty()).map(std::path::PathBuf::from);
    if let Some(tail) = home.as_ref().and_then(|home| m.path.strip_prefix(home).ok()) {
        if tail.as_os_str().is_empty() { "~".into() } else { format!("~/{}", tail.to_string_lossy()) }
    } else { m.path.to_string_lossy().into_owned() }
}
fn sort_label(m: &Model) -> String {
    let name = match m.sort.as_str() { "size" => "Size", "mtime" | "date" => "Modified", "kind" => "Kind", _ => "Name" };
    format!("{name} {}", if m.reverse { "▾" } else { "▴" })
}
// The leading non-tab span preserves the existing mouse hit map; tab() refuses its sentinel index.
pub fn tab_layout(m: &Model, columns: usize) -> Vec<(usize, String)> {
    if m.tabs.len() < 2 { return Vec::new(); }
    let available = columns.saturating_sub(text_width(&sort_label(m)) + 2).max(1);
    let label = |i: usize| format!(" {} {} ", i + 1, m.tabs[i].path.file_name().unwrap_or_default().to_string_lossy());
    let mut first = m.tab;
    let mut used = text_width(&label(first)).min(available);
    while first > 0 && used + text_width(&label(first - 1)) <= available {
        first -= 1;
        used += text_width(&label(first));
    }
    let mut tabs = Vec::new();
    let mut used = 0;
    for i in first..m.tabs.len() {
        if used == available { break; }
        let width = text_width(&label(i)).min(available - used);
        tabs.push((i, ellipsis(&label(i), width)));
        used += width;
    }
    tabs.insert(0, (usize::MAX, " ".repeat(available - used)));
    tabs
}
fn crumbs(path: &str, columns: usize, theme: &Theme) -> String {
    let path = cells::clean(path);
    let split = path.rfind('/').map_or(0, |i| i + 1);
    if text_width(&path) <= columns {
        return format!("{}{}{}{}", theme.meta, &path[..split], theme.foreground, fit(&path[split..], columns - text_width(&path[..split])));
    }
    let current = &path[split..];
    let current_width = text_width(current).min(columns);
    let parent_width = columns - current_width;
    let parent = if parent_width >= 2 { format!("{}/", ellipsis(&path[..split], parent_width - 1)) }
        else { fit("", parent_width) };
    format!("{}{}{}{}", theme.meta, parent, theme.foreground, ellipsis(current, current_width))
}
pub fn header(m: &Model, theme: &Theme, columns: usize) -> String {
    let inner = columns.saturating_sub(2);
    let sort = sort_label(m);
    let sort_width = text_width(&sort).min(inner);
    let room = inner.saturating_sub(sort_width);
    let gap = room.min(2);
    let mut out = format!("{} ", theme.base());
    let tabs = tab_layout(m, inner);
    if tabs.is_empty() { out.push_str(&crumbs(&display_path(m), room - gap, theme)); }
    else {
        for (index, text) in tabs {
            if index == usize::MAX { out.push_str(&crumbs(&display_path(m), text_width(&text), theme)); }
            else {
                out.push_str(if index == m.tab { &theme.tab } else { &theme.background });
                out.push_str(if index == m.tab { &theme.foreground } else { &theme.muted });
                out.push_str(&text);
                out.push_str(&theme.base());
            }
        }
    }
    out.push_str(&format!("{}{}{}{} ", theme.base(), " ".repeat(gap), theme.meta, fit(&sort, sort_width)));
    out
}
pub fn rule(columns: usize, preview: bool, bottom: bool, theme: &Theme) -> String {
    let (left, middle, _) = panes(columns, preview);
    let mut rule = vec!['─'; columns];
    for x in [Some(left), preview.then_some(left + middle + 1)].into_iter().flatten() {
        if let Some(cell) = rule.get_mut(x) { *cell = if bottom { '┴' } else { '┬' }; }
    }
    format!("{}{}{}", theme.border, rule.into_iter().collect::<String>(), theme.base())
}
pub fn focused(m: &Model) -> bool {
    !m.preview_focus && !m.menu && !m.sheet && m.deletion.is_none() && !m.quicklook
        && m.editor.as_ref().map_or(true, |editor| matches!(editor.kind, "filter" | "search"))
}
pub fn thumb(m: &Model, body: usize, now: Instant) -> Option<(usize, usize)> {
    let (first, total) = super::filter::lane(m);
    if total <= body || body == 0 || m.scrolled_at.map_or(true, |at| now.saturating_duration_since(at) >= Duration::from_millis(700)) { return None; }
    let height = (body.saturating_mul(body) / total).max(1).min(body);
    let top = first.min(total - body) * (body - height) / (total - body);
    Some((top, height))
}
pub fn small(columns: usize, lines: usize, theme: &Theme) -> String {
    let message = "Make the window at least 12 columns wide and 5 rows tall.";
    let wrapped = cells::words(message, columns.saturating_sub(1).max(1));
    let start = lines.saturating_sub(wrapped.len()) / 2;
    let mut out = format!("\x1b[H\x1b[2J{}", theme.base());
    for y in 0..lines {
        let text = y.checked_sub(start).and_then(|i| wrapped.get(i)).map(String::as_str).unwrap_or("");
        let line = if wrapped.len() == 1 { cells::centered(text, columns) }
            else if columns > 1 { format!(" {}", fit(text, columns - 1)) } else { fit(text, columns) };
        out.push_str(&format!("\x1b[{};1H{line}", y + 1));
    }
    out
}
pub fn selection_bytes(m: &Model) -> Option<usize> {
    m.selected.iter().try_fold(0usize, |sum, index| m.selected_rows.get(index).or_else(|| m.rows.get(index)).map(|row| sum.saturating_add(row.size)))
}
pub fn footer(m: &Model, theme: &Theme, columns: usize, _elapsed: Duration) -> String {
    let base = theme.base();
    let help = if m.hints.keys.is_empty() { String::new() } else { format!("{} keys", m.hints.keys) };
    let disk = if m.disk_path == m.path && !m.disk_fs.is_empty() { format!("{} · {} free", m.disk_fs, super::render::bytes(m.disk_free)) } else { String::new() };
    let count = if !m.filter.is_empty() {
        format!("Filter {}  {} of {} shown", m.filter, super::render::grouped(m.filter_count()), super::render::grouped(m.total))
    } else if !m.selected.is_empty() {
        format!("{} of {} selected{}", super::render::grouped(m.selected.len()), super::render::grouped(m.total),
            selection_bytes(m).map(|bytes| format!(" · {}", super::render::bytes(bytes))).unwrap_or_default())
    } else { super::render::items(m.total) };
    let mut parts = vec![(theme.foreground.as_str(), count, 0)];
    if !m.error.is_empty() { parts.push((&theme.error, m.error.clone(), 2)); }
    if !m.transfer.is_empty() {
        // A job list of more than one job names its count after the bar, in the muted ink.
        let suffix = if m.job_status.count > 1 { format!(" {}", m.job_status.suffix()) } else { String::new() };
        parts.push((&theme.foreground, m.transfer.strip_suffix(&suffix).unwrap_or(&m.transfer).into(), 2));
        // The job list owns the bar while it names a job; the legacy transfer percent draws only without one.
        let percent = if m.job_status.id == 0 { m.progress_percent } else { m.job_status.percent };
        if let Some(percent) = percent {
            parts.extend(m.job_status.bar(theme, percent));
        }
        if !suffix.is_empty() { parts.push((&theme.muted, suffix.trim_start().into(), 2)); }
    }
    if !m.search.is_empty() { parts.push((&theme.foreground, m.search.clone(), 2)); }
    if m.error.is_empty() && m.transfer.is_empty() && m.search.is_empty() && !m.message.is_empty() { parts.push((&theme.foreground, m.message.clone(), 2)); }
    let left_width = parts.iter().map(|(_, s, gap)| text_width(s) + gap).sum::<usize>();
    let mut right = String::new();
    let mut right_width = 0;
    for (color, text) in [(theme.foreground.as_str(), disk), (&theme.muted, help)] {
        let gap = if right_width == 0 { 0 } else { 3 };
        if text.is_empty() || text_width(&text) + right_width + gap + left_width.min(columns) + 2 > columns { continue; }
        if gap > 0 { right.push_str(&format!("{} · ", theme.muted)); }
        right.push_str(color);
        right.push_str(&text);
        right_width += text_width(&text) + gap;
    }
    let available = columns.saturating_sub(right_width + usize::from(right_width > 0) * 2);
    let mut out = String::new();
    let mut used = 0;
    for (color, text, gap) in parts {
        let gap = if used == 0 { 0 } else { gap.min(available.saturating_sub(used)) };
        out.push_str(&" ".repeat(gap));
        used += gap;
        let width = text_width(&text).min(available.saturating_sub(used));
        out.push_str(color);
        out.push_str(&ellipsis(&text, width));
        out.push_str(&base);
        used += width;
    }
    out.push_str(&" ".repeat(columns.saturating_sub(used + right_width)));
    out.push_str(&right);
    out.push_str(&base);
    out
}

pub fn preview(m: &Model, y: usize, columns: usize, body: usize, theme: &Theme) -> String {
    let padding = pane_padding(columns);
    let width = columns - padding * 2;
    let base = theme.base();
    let mut out = format!("{}{}", base, " ".repeat(padding));
    if m.selected.len() > 1 {
        if y == 0 {
            let mark = if m.marks() { format!("{} ", icons::FILES) } else { String::new() };
            out.push_str(&format!("\x1b[1m{}\x1b[22m", ellipsis(&format!("{mark}{} selected", super::render::items(m.selected.len())), width)));
        } else if y == 1 {
            out.push_str(&theme.meta);
            let facts = selection_bytes(m).map(|n| format!("Combined · {}", super::render::bytes(n))).unwrap_or_default();
            out.push_str(&fit(&facts, width));
        } else if y >= 3 {
            if let Some(row) = m.selected_rows.values().nth(m.preview_scroll + y - 3) {
                out = cells::row(row, columns, Some(&super::render::bytes(row.size)), m.marks(), cells::RowStyle { quiet: true, ..Default::default() }, theme, "");
                return out;
            }
            out.push_str(&theme.meta);
            out.push_str(&fit(&super::render::selection_line(m, y + m.preview_scroll, width), width));
        } else { out.push_str(&fit("", width)); }
    } else if let Some(row) = m.rows.get(&m.cursor).filter(|_| m.current_path().as_ref() == Some(&m.preview_path)) {
        if y == 0 {
            let mark = if m.marks() { format!("{} ", icons::row(row, true)) } else { String::new() };
            out.push_str(&format!("\x1b[1m{}\x1b[22m", ellipsis(&format!("{mark}{}", row.name), width)));
        } else if y == 1 {
            let modified = super::render::modified(row.modified);
            let facts = format!("{} · {}{}", if row.directory { "Folder" } else { &row.kind }, m.row_size(m.cursor, row),
                if modified.is_empty() { String::new() } else { format!(" · {modified}") });
            out.push_str(&theme.meta);
            out.push_str(&ellipsis(&facts, width));
        } else if row.directory && m.preview_rows_generation == m.preview_generation {
            if !m.preview_refusal.is_empty() {
                let height = body.saturating_sub(3);
                let line = y.saturating_sub(3);
                let start = height.saturating_sub(m.preview_line_count) / 2;
                if y >= 3 && line >= start {
                    let index = line - start;
                    if m.preview_scroll + index == 0 {
                        out.push_str(&theme.error);
                        out.push_str(&cells::centered(if m.marks() { icons::LOCK } else { "!" }, width));
                    } else {
                        out.push_str(&cells::centered(m.preview_display.get(index).map(String::as_str).unwrap_or(""), width));
                    }
                } else { out.push_str(&fit("", width)); }
            } else if y >= 3 && m.preview_rows.get(m.preview_scroll + y - 3).is_some() {
                let child = &m.preview_rows[m.preview_scroll + y - 3];
                let size = (child.mode != 0).then(|| super::render::bytes(child.size));
                return cells::row(child, columns, size.as_deref(), m.marks(), cells::RowStyle { quiet: true, ..Default::default() }, theme, "");
            } else {
                out.push_str(&fit(y.checked_sub(3).and_then(|i| m.preview_display.get(i)).map(String::as_str).unwrap_or(""), width));
            }
        } else if m.player.is_some() || m.pdf.is_some() || m.image_file.is_some() {
            let details = m.preview_metadata.len().min(3);
            let start = body.saturating_sub(2 + details + usize::from(m.pdf.is_some()));
            let content = if y == body.saturating_sub(2) {
                m.player.as_ref().map(|p| p.line(width)).or_else(|| m.pdf.as_ref().map(|p| p.line(m.quicklook, width))).unwrap_or_default()
            } else if m.pdf.is_some() && y == body.saturating_sub(3) { m.pdf.as_ref().unwrap().prefix() }
            else if y >= start && y < start + details { m.preview_metadata[y - start].clone() } else { String::new() };
            out.push_str(&theme.meta);
            out.push_str(&fit(&content, width));
        } else if let Some(content) = y.checked_sub(3).and_then(|i| m.preview_display.get(i)) {
            out.push_str(&theme.meta);
            out.push_str(&fit(content, width));
        } else if let Some((number, text)) = y.checked_sub(3 + m.preview_display.len()).and_then(|i| m.text_lines.get(i)) {
            let gutter = m.text_gutter;
            let number = number.map(|n| format!("{:>gutter$}", super::render::grouped(n))).unwrap_or_else(|| " ".repeat(gutter));
            out.push_str(&format!("{}{} {}{}", theme.meta, number, theme.foreground, fit(text, width.saturating_sub(gutter + 1))));
        } else { out.push_str(&fit("", width)); }
    } else { out.push_str(&fit("", width)); }
    out.push_str(&" ".repeat(padding));
    out.push_str(&base);
    out
}

pub fn preview_scroll_limit(m: &Model) -> usize {
    let rows = m.height.saturating_sub(3);
    if rows == 0 { 0 } else { m.preview_line_count.saturating_sub(rows) }
}
fn plain_lines(m: &Model, columns: usize) -> impl Iterator<Item = &str> {
    m.preview.iter().skip(2).filter(|line| !line.is_empty())
        .chain(m.preview_metadata.iter().filter(|line| !line.starts_with("Modified · ")))
        .flat_map(move |line| cells::Wrapped::new(line, columns))
}
fn text_lines(m: &Model, columns: usize) -> impl Iterator<Item = (Option<usize>, &str)> {
    m.preview_body.lines().enumerate().flat_map(move |(i, line)| {
        cells::Wrapped::new(line, columns).enumerate().map(move |(part, text)| ((part == 0).then_some(i + 1), text))
    })
}
// One body layout owns visible rows and scrolling below the fixed three-row header.
pub fn prepare(m: &mut Model) {
    let columns = if m.quicklook || m.preview_visible { preview_area(m.columns, m.quicklook).1 } else { 0 };
    let changed = m.preview_width != columns || m.preview_layout_generation != m.preview_generation || m.preview_body_bytes != m.preview_body.len();
    if m.selected.len() <= 1 && !changed && m.preview_layout_scroll == m.preview_scroll && m.preview_layout_height == m.height { return; }
    let rows = m.height.saturating_sub(3);
    let folder = m.rows.get(&m.cursor).is_some_and(|row| row.directory) && m.preview_rows_generation == m.preview_generation;
    let refusal = folder && !m.preview_refusal.is_empty();
    let graphical = m.player.is_some() || m.pdf.is_some() || m.image_file.is_some();
    m.text_gutter = super::render::grouped(m.preview_body.lines().count()).len();
    let text_width = columns.saturating_sub(m.text_gutter + 1);
    if changed || m.selected.len() > 1 {
        m.preview_line_count = if m.selected.len() > 1 { m.selected_rows.len() + usize::from(m.selected_rows.len() < m.selected.len()) }
        else if refusal { 1 + cells::words(&m.preview_refusal, columns).len() }
        else if folder && !m.preview_rows.is_empty() { m.preview_rows.len() }
        else if graphical { 0 }
        else { plain_lines(m, columns).count() + text_lines(m, text_width).count() };
    }
    m.preview_scroll = m.preview_scroll.min(preview_scroll_limit(m));
    m.preview_display = if refusal {
        std::iter::once(String::new()).chain(cells::words(&m.preview_refusal, columns)).skip(m.preview_scroll).take(rows).collect()
    } else if m.selected.len() > 1 || graphical || (folder && !m.preview_rows.is_empty()) { Vec::new() }
        else { plain_lines(m, columns).skip(m.preview_scroll).take(rows).map(cells::clean).collect() };
    m.text_lines = if m.preview_body.is_empty() || m.selected.len() > 1 || folder || graphical { Vec::new() }
        else {
            let skip = m.preview_scroll.saturating_sub(plain_lines(m, columns).count());
            text_lines(m, text_width).skip(skip).take(rows.saturating_sub(m.preview_display.len())).map(|(number, line)| (number, cells::clean(line))).collect()
        };
    m.preview_body_bytes = m.preview_body.len();
    m.preview_width = columns;
    m.preview_layout_generation = m.preview_generation;
    m.preview_layout_scroll = m.preview_scroll;
    m.preview_layout_height = m.height;
}

pub struct Live {
    pub theme: Theme,
    source: String,
    depth: Depth,
    probe: icons::Probe,
    watcher: super::theme::Watch,
}
impl Live {
    pub fn new(model: &mut Model) -> Self {
        model.font_marks = icons::builtin();
        model.force_text_marks = std::env::var_os("FLEA_TUI_TEST_NO_ICONS").is_some_and(|value| value == "1");
        Self { theme: Theme::with_depth("", Depth::Sixteen), source: String::new(), depth: Depth::Sixteen,
            probe: icons::Probe::start(), watcher: super::theme::Watch::start(Theme::path()) }
    }
    pub fn prepare(&mut self, model: &mut Model, wire: &mut super::wire::Wire) -> std::io::Result<()> {
        self.poll(model);
        model.request_sizes(wire, Instant::now())?;
        super::preview::load(model, false);
        super::preview::poll(model);
        super::preview::request(model, wire)?;
        super::preview::layout(model);
        Ok(())
    }
    fn poll(&mut self, model: &mut Model) {
        let mut changed = false;
        if let Ok((marks, depth)) = self.probe.result.try_recv() { model.font_marks = marks; self.depth = depth; changed = true; }
        while let Ok(text) = self.watcher.updates.try_recv() { self.source = text; changed = true; }
        if changed { self.theme = Theme::with_depth(&self.source, self.depth); }
    }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
