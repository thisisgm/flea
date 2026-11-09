use super::{cells::{self, ellipsis, fit, text_width}, icons, model::Model, theme::{Depth, Theme}};
use std::time::{Duration, Instant};

pub const BODY_ROW: usize = 3;
pub const CHROME_ROWS: usize = 4;
pub fn rtl(m: &Model) -> bool { m.i18n.locale().right_to_left && cells::mirrors("columns") }
// HIG tui-lang-ar keeps the rail, list and preview in the same order as the English board.
pub fn visual_start(_m: &Model, start: usize, _width: usize) -> usize { start }
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
    if let Some(place) = super::trashview::place(m) { return place; }
    short_path(&m.path)
}
pub(super) fn short_path(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").filter(|s| !s.is_empty()).map(std::path::PathBuf::from);
    if let Some(tail) = home.as_ref().and_then(|home| path.strip_prefix(home).ok()) {
        if tail.as_os_str().is_empty() { "~".into() } else { format!("~/{}", tail.to_string_lossy()) }
    } else { path.to_string_lossy().into_owned() }
}
fn sort_label(m: &Model) -> String {
    if let Some(sort) = super::trashview::sort(m) { return sort; }
    let name = match m.sort.as_str() { "size" => "window.column_size", "mtime" | "date" => "window.column_modified", "kind" => "list.column_kind", "btime" => "properties.birth", "extension" => "tui.sort.extension", "random" => "tui.sort.random", _ => "window.column_name" };
    format!("{} {}", m.i18n.tr(name), if m.reverse { "▾" } else { "▴" })
}
// The arrow owns its cell after the isolated label, even when the label reads right to left.
fn sort_cells(sort: &str, width: usize) -> String {
    if let Some((label, arrow @ ("▴" | "▾"))) = sort.rsplit_once(' ') {
        if width >= 2 { return format!("{} {arrow}", fit(label, width - 2)); }
        return fit(label, width);
    }
    fit(sort, width)
}
// The leading non-tab span preserves the existing mouse hit map; tab() refuses its sentinel index.
pub fn tab_layout(m: &Model, columns: usize) -> Vec<(usize, String)> {
    if m.tabs.len() < 2 { return Vec::new(); }
    let available = columns.saturating_sub(text_width(&sort_label(m)) + 2).max(1);
    let label = |i: usize| format!(" {} {} ", m.i18n.locale().format_count((i + 1) as i64), m.tabs[i].path.file_name().unwrap_or_default().to_string_lossy());
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
    out.push_str(&format!("{}{}{}{} ", theme.base(), " ".repeat(gap), theme.meta, sort_cells(&sort, sort_width)));
    out
}
#[cfg(test)]
#[allow(dead_code)]
pub fn rule(columns: usize, preview: bool, bottom: bool, theme: &Theme) -> String { rule_direction(columns, preview, bottom, theme, false) }
pub fn rule_for(m: &Model, columns: usize, bottom: bool, theme: &Theme) -> String { rule_direction(columns, m.preview_visible, bottom, theme, false) }
fn rule_direction(columns: usize, preview: bool, bottom: bool, theme: &Theme, rtl: bool) -> String {
    let (left, middle, _) = panes(columns, preview);
    let mut rule = vec!['─'; columns];
    for x in [Some(left), preview.then_some(left + middle + 1)].into_iter().flatten() {
        let x = if rtl { columns.saturating_sub(x + 1) } else { x };
        if let Some(cell) = rule.get_mut(x) { *cell = if bottom { '┴' } else { '┬' }; }
    }
    format!("{}{}{}", theme.border, rule.into_iter().collect::<String>(), theme.base())
}
pub fn focused(m: &Model) -> bool {
    m.combining.card.is_none() && m.converting.card.is_none() && m.printing.card.is_none() && !m.preview_focus && !m.menu && !m.sheet && m.deletion.is_none() && !m.quicklook
        && m.editor.as_ref().map_or(m.prompts.question.is_none(), |editor| matches!(editor.kind, "filter" | "search"))
}
// An empty listing leaves the preview nothing to take keys unless a player or document is still drawn; an unfetched row counts.
pub fn preview_focusable(m: &Model) -> bool {
    m.preview_visible && (m.total > 0 || m.player.is_some() || m.pdf.is_some())
}
pub fn release_preview_focus(m: &mut Model) {
    if m.preview_focus && !m.quicklook && !preview_focusable(m) {
        m.preview_focus = false;
    }
}
pub fn thumb(m: &Model, body: usize, now: Instant) -> Option<(usize, usize)> {
    let (first, total) = super::filter::lane(m);
    let first = first.saturating_mul(super::contents::row_height(m));
    let total = total.saturating_mul(super::contents::row_height(m));
    if total <= body || body == 0 || m.scrolled_at.map_or(true, |at| now.saturating_duration_since(at) >= Duration::from_millis(700)) { return None; }
    let height = (body.saturating_mul(body) / total).max(1).min(body);
    let top = first.min(total - body) * (body - height) / (total - body);
    Some((top, height))
}
#[cfg(test)]
pub fn small(columns: usize, lines: usize, theme: &Theme) -> String {
    small_localized(columns, lines, theme, &crate::i18n::test_catalogs::english())
}
pub fn small_localized(columns: usize, lines: usize, theme: &Theme, i18n: &crate::i18n::I18n) -> String {
    let message = i18n.tr("tui.layout.make_the_window_at_least_12_columns_wide_and_5_rows_tall");
    let wrapped = cells::words(&message, columns.saturating_sub(1).max(1));
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
    let help = if m.hints.keys.is_empty() { String::new() } else { m.i18n.tr_args("tui.frame.keys", &[&m.hints.keys]) };
    let disk = if (m.disk_path == m.path || super::trashview::opened_from(m) == Some(&m.disk_path)) && !m.disk_fs.is_empty() { m.i18n.tr_args("status.host.free", &[&m.disk_fs, &m.i18n.locale().format_size(m.disk_free as u64)]) } else { String::new() };
    let count = if !m.selected.is_empty() {
        format!("{}{}", m.i18n.trn_args("status.host.selected", m.selected.len() as i64, &[&m.i18n.locale().format_count(m.total as i64)]),
            selection_bytes(m).map(|bytes| format!(" · {}", m.i18n.locale().format_size(bytes as u64))).unwrap_or_default())
    } else { super::words::items(&m.i18n, m.total) };
    let mut parts = super::prompts::lead(m, theme, count);
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
    if m.error.is_empty() && m.transfer.is_empty() && m.search.is_empty() && !m.message.is_empty() { parts.extend(super::prompts::message(m, theme)); }
    let left_width = parts.iter().map(|(_, s, gap)| text_width(s) + gap).sum::<usize>();
    let mut right = String::new();
    let mut right_width = 0;
    for (color, text) in [(theme.foreground.as_str(), disk), (&theme.muted, help)] {
        let gap = if right_width == 0 { 0 } else { 3 };
        if text.is_empty() || text_width(&text) + right_width + gap + left_width.min(columns) + 2 > columns { continue; }
        if gap > 0 { right.push_str(&format!("{} · ", theme.muted)); }
        right.push_str(color);
        right.push_str(&fit(&text, text_width(&text)));
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

// The preview pane's drawing and layout live in pane.rs.
pub use super::pane::{prepare, preview_scroll_limit};
#[cfg(test)]
pub use super::pane::preview;

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
        let state = &mut model.preview_state;
        state.colors = super::still::colors(self.depth);
        if state.ground != self.theme.ground { state.ground = self.theme.ground.clone(); }
        if model.quicklook != state.probed { state.probed = model.quicklook; if model.quicklook { super::render::cards::probe(&model.rounded); } }
        model.request_sizes(wire, Instant::now())?;
        super::custom_preview::settle(model, wire)?;
        super::preview::load(model, false);
        super::preview::poll(model);
        super::custom_preview::prepare(model);
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
