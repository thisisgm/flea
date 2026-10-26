use super::{cells::{self, ellipsis, fit, name_title, text_width}, icons, model::Model, picture::{self, Slot}, theme::Theme};
use super::layout::{pane_padding, preview_area, selection_bytes};

// The roles a preview control draws in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink {
    Foreground,
    Muted,
    Meta,
    Frame,
}

// Where a preview body draws: the full width with its padding, the rows under the head, and whether it sits in a card.
#[derive(Clone, Copy)]
pub struct Area {
    pub columns: usize,
    pub padding: usize,
    pub height: usize,
    pub card: bool,
}
impl Area {
    pub fn width(&self) -> usize {
        self.columns.saturating_sub(self.padding * 2)
    }
}

pub fn paint(runs: &[(Ink, String)], width: usize, theme: &Theme) -> String {
    let mut out = String::new();
    let mut used = 0;
    for (ink, text) in runs {
        let text = fit(text, text_width(text).min(width - used));
        out.push_str(match ink { Ink::Foreground => &theme.foreground, Ink::Muted => &theme.muted, Ink::Meta => &theme.meta, Ink::Frame => &theme.track });
        used += text_width(&text);
        out.push_str(&text);
    }
    out.push_str(&theme.foreground);
    out.push_str(&" ".repeat(width - used));
    out
}

fn padded(inner: &str, area: &Area, theme: &Theme) -> String {
    let base = theme.base();
    let pad = " ".repeat(area.padding);
    format!("{base}{pad}{inner}{base}{pad}{base}")
}

// One row of the preview pane, asking the no-match gate itself, for tests that draw one row.
#[cfg(test)]
pub fn preview(m: &Model, y: usize, columns: usize, height: usize, theme: &Theme) -> String {
    preview_row(m, y, columns, height, theme, super::prompts::previewed(m))
}
// One row of the preview pane, the head then the body the file's kind draws, with the no-match gate already answered so a frame asks it once.
pub fn preview_row(m: &Model, y: usize, columns: usize, height: usize, theme: &Theme, previewed: bool) -> String {
    let padding = pane_padding(columns);
    let area = Area { columns, padding, height: height.saturating_sub(3), card: false };
    let width = area.width();
    if m.selected.len() > 1 {
        return selection(m, y, &area, theme);
    }
    if let Some(line) = super::trashview::preview(m, y, width, theme) {
        return padded(&line, &area, theme);
    }
    let Some(row) = m.rows.get(&m.cursor).filter(|_| m.current_path().as_ref() == Some(&m.preview_path) && previewed) else {
        return padded(&fit("", width), &area, theme);
    };
    let state = &m.preview_state;
    let inner = match y {
        0 => {
            let mark = if m.marks() { format!("{} ", icons::row(row, true)) } else { String::new() };
            let leaf = row.name.rsplit('/').next().unwrap_or(&row.name);
            let head = (text_width(&mark) + text_width(leaf)).min(width);
            let name = name_title(&mark, leaf, head);
            let target = if row.link.is_empty() { String::new() } else { format!(" → {}", row.link) };
            format!("{}{name}{}{}", theme.foreground, theme.meta, ellipsis(&target, width - head))
        }
        1 => {
            let modified = super::render::modified(row.modified);
            let facts = format!("{} · {}{}", if row.directory { "Folder" } else { &row.kind }, m.row_size(m.cursor, row),
                if modified.is_empty() { String::new() } else { format!(" · {modified}") });
            format!("{}{}", theme.meta, ellipsis(&facts, width))
        }
        2 => format!("{}{}", theme.meta, ellipsis(&super::pane_state::archive_line(state).filter(|_| state.unshown.is_none()).unwrap_or_default(), width)),
        _ => return body(m, y - 3, &area, theme),
    };
    padded(&inner, &area, theme)
}

// Row i under a head, whole with its padding: a folder's rows, a can't-show state, an archive's entries, a picture's stack or text.
pub fn body(m: &Model, i: usize, area: &Area, theme: &Theme) -> String {
    let width = area.width();
    let Some(row) = m.rows.get(&m.cursor) else { return padded(&fit("", width), area, theme) };
    let state = &m.preview_state;
    if row.directory && m.preview_rows_generation == m.preview_generation {
        return folder(m, i, area, theme);
    }
    if let Some(unshown) = &state.unshown {
        return padded(&unshown_row(unshown, state.unshown_mark, i, area, theme, &state.i18n, m.marks()), area, theme);
    }
    if let Some(archive) = &state.archive {
        let first = usize::from(!area.card);
        let index = (m.preview_scroll + i).checked_sub(first);
        if let Some((entry, size)) = index.and_then(|index| archive.rows.get(index)) {
            let size = size.map(super::render::bytes);
            return cells::row(entry, area.columns, size.as_deref(), m.marks(), cells::RowStyle { quiet: true, ..Default::default() }, theme, "");
        }
        let more = archive.entries.saturating_sub(archive.rows.len());
        let line = if index == Some(archive.rows.len()) && more > 0 { state.i18n.trn("preview.more", more as i64) } else { String::new() };
        return padded(&format!("{}{}", theme.meta, fit(&line, width)), area, theme);
    }
    if m.player.is_some() || m.pdf.is_some() || m.image_file.is_some() || super::pane_state::kind(row) == super::pane_state::Kind::Model3d {
        return padded(&stacked(m, i, area, theme), area, theme);
    }
    if let Some(content) = m.preview_display.get(i) {
        return padded(&format!("{}{}", theme.meta, fit(content, width)), area, theme);
    }
    let index = i - m.preview_display.len().min(i);
    if let Some((number, text)) = m.text_lines.get(index) {
        let gutter = m.text_gutter;
        let number = number.map(|n| format!("{:>gutter$}", super::render::grouped(n))).unwrap_or_else(|| " ".repeat(gutter));
        let text = super::syntax::paint(m, index, text, width.saturating_sub(gutter + 1), theme);
        return padded(&format!("{}{number} {text}", theme.meta), area, theme);
    }
    padded(&fit("", width), area, theme)
}

fn stacked(m: &Model, i: usize, area: &Area, theme: &Theme) -> String {
    let width = area.width();
    let i18n = &m.preview_state.i18n;
    let focused = m.preview_focus || m.quicklook;
    match picture::slot(m, i, area) {
        // Behind a card (Quick Look, the keys sheet, Properties or the delete card) the pane's picture is cleared; a terminal could only dim it by sending it again.
        Slot::Picture(y) => match picture::grid(m).filter(|_| area.card || !(m.quicklook || m.sheet || m.deletion.is_some())) {
            Some(grid) => {
                let offset = if area.card { width.saturating_sub(grid.columns) / 2 } else { 0 };
                format!("{}{}", " ".repeat(offset), super::still::row(grid, y, width - offset, theme))
            }
            None => fit("", width),
        },
        Slot::Player => m.player.as_ref().map(|p| paint(&p.runs(width, focused, i18n), width, theme)).unwrap_or_else(|| fit("", width)),
        Slot::Page => format!("{}{}", theme.meta, fit(&m.pdf.as_ref().map(|p| p.page_line(i18n)).unwrap_or_default(), width)),
        Slot::Controls => m.pdf.as_ref().map(|p| paint(&p.runs(focused, m.quicklook, width), width, theme)).unwrap_or_else(|| fit("", width)),
        Slot::Detail(index) => format!("{}{}", theme.meta, ellipsis(&m.preview_metadata[index], width)),
        Slot::Blank => fit("", width),
    }
}

// The error alert every can't-show row draws by default; an Office mark names its own glyph and tone.
pub const ALERT: &str = "\u{ea6c}";
// The can't-show state, centred under the head: the alert mark in the error role, one sentence, then Open With when offered.
fn unshown_row(unshown: &super::pane_state::Unshown, mark: Option<super::pane_state::Mark>, i: usize, area: &Area, theme: &Theme, i18n: &crate::i18n::I18n, marks: bool) -> String {
    let width = area.width();
    let rows = if unshown.open_with { 4 } else { 2 };
    let top = area.height.saturating_sub(rows) / 2;
    let (glyph, tone) = match mark {
        Some(mark) => (mark.glyph, if mark.error { &theme.error } else { &theme.muted }),
        None => (ALERT, &theme.error),
    };
    let label = if marks { glyph.to_string() } else if mark.map_or(true, |mark| mark.error) { "!".to_string() } else { String::new() };
    match i.checked_sub(top) {
        Some(0) => format!("{tone}{}", cells::centered(&label, width)),
        Some(1) => format!("{}{}", theme.foreground, cells::centered(&unshown.sentence, width)),
        Some(3) if unshown.open_with && width >= 5 => {
            let label = i18n.tr("preview.openWith");
            let label = ellipsis(&label, text_width(&label).min(width - 4));
            let left = (width - text_width(&label) - 4) / 2;
            let right = width - left - text_width(&label) - 4;
            format!("{}{}[ {}{label}{} ]{}", " ".repeat(left), theme.frame, theme.foreground, theme.frame, " ".repeat(right))
        }
        _ => fit("", width),
    }
}

fn folder(m: &Model, i: usize, area: &Area, theme: &Theme) -> String {
    let width = area.width();
    if !m.preview_refusal.is_empty() {
        let start = area.height.saturating_sub(m.preview_line_count) / 2;
        let inner = match i.checked_sub(start) {
            Some(index) if m.preview_scroll + index == 0 => format!("{}{}", theme.error, cells::centered(if m.marks() { icons::LOCK } else { "!" }, width)),
            Some(index) => cells::centered(m.preview_display.get(index).map(String::as_str).unwrap_or(""), width),
            None => fit("", width),
        };
        return padded(&inner, area, theme);
    }
    if let Some(child) = m.preview_rows.get(m.preview_scroll + i) {
        let size = (child.mode != 0).then(|| super::render::bytes(child.size));
        return cells::row(child, area.columns, size.as_deref(), m.marks(), cells::RowStyle { quiet: true, ..Default::default() }, theme, "");
    }
    padded(&fit(m.preview_display.get(i).map(String::as_str).unwrap_or(""), width), area, theme)
}

fn selection(m: &Model, y: usize, area: &Area, theme: &Theme) -> String {
    let width = area.width();
    let inner = if y == 0 {
        let mark = if m.marks() { format!("{} ", icons::FILES) } else { String::new() };
        name_title(&mark, &format!("{} selected", super::render::items(m.selected.len())), width)
    } else if y == 1 {
        let facts = selection_bytes(m).map(|n| format!("Combined · {}", super::render::bytes(n))).unwrap_or_default();
        format!("{}{}", theme.meta, fit(&facts, width))
    } else if y >= 3 {
        if let Some(row) = m.selected_rows.values().nth(m.preview_scroll + y - 3) {
            return cells::row(row, area.columns, Some(&super::render::bytes(row.size)), m.marks(), cells::RowStyle { quiet: true, ..Default::default() }, theme, "");
        }
        format!("{}{}", theme.meta, fit(&super::render::selection_line(m, y + m.preview_scroll, width), width))
    } else {
        fit("", width)
    };
    padded(&inner, area, theme)
}

// The rows a body scrolls through, in the pane or the Quick Look card.
pub fn rows(m: &Model) -> usize {
    if m.quicklook { super::look::text_rows(m.height) } else { m.height.saturating_sub(3) }
}
pub fn preview_scroll_limit(m: &Model) -> usize {
    let rows = rows(m);
    if rows == 0 { 0 } else { m.preview_line_count.saturating_sub(rows) }
}
fn plain_lines(m: &Model, columns: usize) -> impl Iterator<Item = &str> {
    m.preview.iter().skip(2).filter(|line| !line.is_empty())
        .chain(m.preview_metadata.iter().filter(|line| !line.starts_with("Modified · ")))
        .flat_map(move |line| cells::Wrapped::new(line, columns))
}
// Each wrapped part with its line number on the first part, and its source line and byte offset for syntax runs.
fn text_lines(m: &Model, columns: usize) -> impl Iterator<Item = (Option<usize>, &str, usize, usize)> {
    m.preview_body.lines().enumerate().flat_map(move |(i, line)| {
        cells::Wrapped::new(line, columns).enumerate()
            .map(move |(part, text)| ((part == 0).then_some(i + 1), text, i + 1, text.as_ptr() as usize - line.as_ptr() as usize))
    })
}
// One body layout owns visible rows and scrolling below the fixed three-row header.
pub fn prepare(m: &mut Model) {
    let columns = if m.quicklook { super::look::area(m.columns, m.height).width() } else if m.preview_visible { preview_area(m.columns, false).1 } else { 0 };
    let changed = m.preview_width != columns || m.preview_layout_generation != m.preview_generation || m.preview_body_bytes != m.preview_body.len();
    if m.selected.len() <= 1 && !changed && m.preview_layout_scroll == m.preview_scroll && m.preview_layout_height == m.height { return; }
    let rows = rows(m);
    let folder = m.rows.get(&m.cursor).is_some_and(|row| row.directory) && m.preview_rows_generation == m.preview_generation;
    let refusal = folder && !m.preview_refusal.is_empty();
    let graphical = m.player.is_some() || m.pdf.is_some() || m.image_file.is_some() || m.preview_state.unshown.is_some()
        || m.rows.get(&m.cursor).is_some_and(|r| super::pane_state::kind(r) == super::pane_state::Kind::Model3d);
    let archive = m.preview_state.archive.as_ref().map(|a| a.rows.len() + usize::from(a.entries > a.rows.len()) + usize::from(!m.quicklook));
    m.text_gutter = super::render::grouped(m.preview_body.lines().count()).len();
    let text_width = columns.saturating_sub(m.text_gutter + 1);
    if changed || m.selected.len() > 1 || archive.is_some() {
        m.preview_line_count = if m.selected.len() > 1 { m.selected_rows.len() + usize::from(m.selected_rows.len() < m.selected.len()) }
        else if refusal { 1 + cells::words(&m.preview_refusal, columns).len() }
        else if folder && !m.preview_rows.is_empty() { m.preview_rows.len() }
        else if graphical { 0 }
        else if let Some(count) = archive { count }
        else { plain_lines(m, columns).count() + text_lines(m, text_width).count() };
    }
    m.preview_scroll = m.preview_scroll.min(preview_scroll_limit(m));
    m.preview_display = if refusal {
        std::iter::once(String::new()).chain(cells::words(&m.preview_refusal, columns)).skip(m.preview_scroll).take(rows).collect()
    } else if m.selected.len() > 1 || graphical || archive.is_some() || (folder && !m.preview_rows.is_empty()) { Vec::new() }
        else { plain_lines(m, columns).skip(m.preview_scroll).take(rows).map(cells::clean).collect() };
    let parts: Vec<(Option<usize>, String, usize, usize)> = if m.preview_body.is_empty() || m.selected.len() > 1 || folder || graphical { Vec::new() }
        else {
            let skip = m.preview_scroll.saturating_sub(plain_lines(m, columns).count());
            text_lines(m, text_width).skip(skip).take(rows.saturating_sub(m.preview_display.len())).map(|(number, line, source, offset)| (number, cells::clean(line), source, offset)).collect()
        };
    m.preview_state.code.parts = parts.iter().map(|(_, _, source, offset)| (*source, *offset)).collect();
    m.text_lines = parts.into_iter().map(|(number, line, _, _)| (number, line)).collect();
    m.preview_body_bytes = m.preview_body.len();
    m.preview_width = columns;
    m.preview_layout_generation = m.preview_generation;
    m.preview_layout_scroll = m.preview_scroll;
    m.preview_layout_height = m.height;
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "pane_title_tests.rs"]
mod title_tests;
