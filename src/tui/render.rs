use super::{cells::{self, RowStyle}, layout, keymap::Map, model::Model, theme::Theme};
use std::io::{self, Write};
pub use super::cells::{clean, safe, text_width, fit};
pub use super::layout::{panes, BODY_ROW, CHROME_ROWS, body_height, preview_area, tab_layout};
#[path = "cards.rs"]
pub(super) mod cards;
#[path = "key_sheet.rs"]
pub(super) mod key_sheet;
#[path = "menu_layout.rs"]
pub(super) mod menu_layout;
#[path = "menu_run.rs"]
pub(super) mod menu_run;

#[cfg(test)]
pub fn bytes(n: usize) -> String { crate::i18n::Locale::default().format_size(n as u64) }
#[cfg(test)]
pub fn items(n: usize) -> String { super::words::items(&crate::i18n::test_catalogs::english(), n) }
#[cfg(test)]
pub fn grouped(n: usize) -> String { crate::i18n::Locale::default().format_count(n.min(i64::MAX as usize) as i64) }
pub fn draw(m: &Model, theme: &Theme, map: &Map, columns: usize, lines: usize,
    _cell_pixels: Option<(usize, usize)>, elapsed: std::time::Duration, last: &mut String) -> io::Result<bool> {
    if m.find.waiting || m.find.cancelling { return Ok(false); }
    let out = super::look::frame(m, theme, map, columns, lines, elapsed);
    present(out, last, &mut io::stdout().lock())
}
pub(super) fn present(out: String, last: &mut String, writer: &mut impl Write) -> io::Result<bool> {
    if *last == out { return Ok(false); }
    write!(writer, "{}\x1b[0m", cells::visual::frame(&out))?;
    writer.flush()?;
    *last = out;
    Ok(true)
}
pub(super) fn frame(m: &Model, theme: &Theme, map: &Map, columns: usize, lines: usize, elapsed: std::time::Duration) -> String {
    frame_at(m, theme, map, columns, lines, elapsed, std::time::Instant::now())
}
pub(super) fn frame_at(m: &Model, theme: &Theme, map: &Map, columns: usize, lines: usize,
    elapsed: std::time::Duration, now: std::time::Instant) -> String {
    if columns < 12 || lines <= CHROME_ROWS { return layout::small_localized(columns, lines, theme, &m.i18n) + &super::prompts::caret(None, lines); }
    // A card or the keys sheet is the one lit surface: everything behind it draws in the scrim's inks.
    let lit = theme;
    let theme = if m.sheet || m.deletion.is_some() || (m.printing.card.is_some() || m.converting.card.is_some() || m.combining.card.is_some()) { theme.dim.as_deref().unwrap_or(theme) } else { theme };
    let (left, middle, right) = panes(columns, m.preview_visible);
    let body = body_height(lines);
    let base = theme.base();
    let visible = m.visible();
    let previewed = m.preview_visible && super::prompts::previewed(m);
    let mut out = format!("\x1b[H{}\x1b[2;1H{}", layout::header(m, theme, columns), layout::rule_for(m, columns, false, theme));
    for y in 0..body {
        out.push_str(&format!("\x1b[{};{}H{}", y + BODY_ROW, layout::visual_start(m, 0, left) + 1, base));
        if let Some(parent) = m.parents.get(y).filter(|_| !super::trashview::hides(m)) {
            let current = m.path.file_name().is_some_and(|name| name == parent.name.as_str());
            out.push_str(&cells::row(parent, left, None, m.marks(), RowStyle { cursor: current, quiet: true, parent: true, rtl: layout::rtl(m), ..Default::default() }, theme, ""));
        } else { out.push_str(&" ".repeat(left)); }
        out.push_str(&format!("\x1b[{};{}H{}│{}", y + BODY_ROW, layout::visual_start(m, left, 1) + 1, theme.border, base));
        out.push_str(&format!("\x1b[{};{}H", y + BODY_ROW, layout::visual_start(m, left + 1, middle) + 1));
        if let Some((index, entry)) = visible.get(y / super::contents::row_height(m)).filter(|_| y / super::contents::row_height(m) < body / super::contents::row_height(m)).and_then(|index| m.rows.get(index).map(|entry| (*index, entry))) {
            let style = RowStyle { cursor: index == m.cursor, focused: layout::focused(m), marked: m.selected.contains(&index), dimmed: entry.mode == 0, rtl: layout::rtl(m), ..Default::default() };
            if super::contents::row_height(m) == 2 && y % 2 == 1 {
                out.push_str(&contents_line(m, index, middle, style, theme));
            } else {
                let query = if !m.filter.is_empty() { &m.filter } else if super::contents::row_height(m) == 1 { &m.search_query } else { "" };
                if super::contents::row_height(m) == 2 {
                    out.push_str(&contents_row(m, entry, middle, &super::trashview::detail(m, index, entry), style, theme));
                } else { out.push_str(&cells::row(entry, middle, Some(&super::trashview::detail(m, index, entry)), m.marks(), style, theme, query)); }
            }
        } else if !m.search.is_empty() && !m.searching && m.total == 0 {
            out.push_str(&theme.meta);
            out.push_str(&super::empty::no_match(y, body, middle, m.marks(), &m.prompts.i18n.tr_args("filter.nothing", &[&m.search_query])));
        } else if super::empty::shown(m, columns, lines) {
            out.push_str(&theme.meta);
            out.push_str(&super::empty::localized(y, body, middle, m.marks(), now.saturating_duration_since(m.empty_at), &m.i18n));
        } else if !m.filter.is_empty() && visible.is_empty() {
            out.push_str(&theme.meta);
            out.push_str(&super::empty::no_match(y, body, middle, m.marks(), &m.prompts.i18n.tr_args("filter.nothing", &[&m.filter])));
        } else {
            out.push_str(&fit(&if m.pending.is_some() && y == body / 2 { m.i18n.tr("preview.loading") } else { String::new() }, middle));
        }
        out.push_str(&base);
        if m.preview_visible {
            out.push_str(&format!("\x1b[{};{}H{}│{}", y + BODY_ROW, layout::visual_start(m, left + middle + 1, 1) + 1, theme.border, base));
            out.push_str(&format!("\x1b[{};{}H", y + BODY_ROW, layout::visual_start(m, left + middle + 2, right) + 1));
            out.push_str(&super::pane::preview_row(m, y, right, body, theme, previewed));
        }
    }
    if let Some((top, height)) = layout::thumb(m, body, std::time::Instant::now()) {
        // The heavy thumb rides the listing's separator in the frame ink, as the tui-long board draws it; with no preview, the last cell.
        let column = left + middle + if m.preview_visible { 2 } else { 1 };
        for y in top..top + height { out.push_str(&format!("\x1b[{};{}H{}┃{}", BODY_ROW + y, layout::visual_start(m, column - 1, 1) + 1, theme.frame, base)); }
    }
    out.push_str(&format!("\x1b[{};1H{}\x1b[{};1H{} ", lines - 1, layout::rule_for(m, columns, true, theme), lines, base));
    let (status, caret) = super::prompts::status(m, theme, columns.saturating_sub(2), elapsed);
    out.push_str(&format!("{status}{base} "));
    if m.properties.is_some() { cards::draw_properties(&mut out, m, lit); }
    else if m.sheet { key_sheet::draw(&mut out, m, lit, map); }
    if m.menu { menu_layout::draw(&mut out, m, lit, map); }
    if m.combining.card.is_some() { super::combine::draw(&mut out, m, lit); }
    if m.converting.card.is_some() { super::convert::draw(&mut out, m, lit); }
    if m.printing.card.is_some() { super::print::draw(&mut out, m, lit); }
    if m.deletion.is_some() { cards::draw_delete(&mut out, m, lit); }
    out.push_str(&super::prompts::caret(caret, lines));
    out
}
fn contents_gutter(m: &Model) -> usize {
    m.find.matches.values().map(|hit| text_width(&m.prompts.i18n.locale().format_count(hit.line_no as i64)) + 1).max().unwrap_or(3).max(3)
}
fn contents_line(m: &Model, index: usize, columns: usize, style: RowStyle, theme: &Theme) -> String {
    let Some(hit) = m.find.matches.get(&index) else { return fit("", columns); };
    let fill = if style.cursor { if style.focused { &theme.cursor } else { &theme.selected } }
        else if style.marked { &theme.marked } else { &theme.background };
    let reset = format!("\x1b[0m{fill}{}{}", theme.meta, if style.cursor && theme.depth == super::theme::Depth::Sixteen { "\x1b[7m" } else { "" });
    let number = m.prompts.i18n.locale().format_count(hit.line_no as i64);
    let gutter = contents_gutter(m).min(columns);
    let prefix = format!("{}{} ", " ".repeat(gutter.saturating_sub(text_width(&number) + 1)), number);
    let line = cells::ellipsis(&hit.line, columns.saturating_sub(gutter));
    let query = m.find.active.as_ref().map_or("", |q| q.query.as_str());
    let mut out = format!("{reset}{}", fit(&prefix, gutter));
    let matched = (!query.is_empty()).then(|| hit.line.find(query)).flatten()
        .map(|start| clean(&hit.line[..start]).len()).filter(|start| line.get(*start..).is_some_and(|tail| tail.starts_with(query)));
    if let Some(start) = matched {
        let end = start + query.len();
        out.push_str(&line[..start]);
        out.push_str(if style.cursor || style.marked { "\x1b[1m" } else { &theme.match_fill });
        out.push_str(&line[start..end]);
        out.push_str(&reset);
        out.push_str(&line[end..]);
    } else { out.push_str(&line); }
    out.push_str(&theme.base());
    out
}
fn contents_row(m: &Model, entry: &super::model::Row, columns: usize, size: &str, style: RowStyle, theme: &Theme) -> String {
    let pad = contents_gutter(m).saturating_sub(3);
    let mut entry = entry.clone();
    let (folder, leaf) = entry.name.rsplit_once('/').unwrap_or(("", &entry.name));
    entry.name = format!("{}{}{}", if folder.is_empty() { String::new() } else { format!("{folder}/") }, " ".repeat(pad), leaf);
    cells::row(&entry, columns, Some(size), m.marks(), style, theme, "")
}
#[cfg(test)]
pub(super) fn selection_line(m: &Model, y: usize, _columns: usize) -> String {
    selection_spans(m, y).into_iter().map(|(_, text)| text).collect()
}
// The selection list's count and size lines as typed runs; loaded rows draw through cells::row instead.
pub(super) fn selection_spans(m: &Model, y: usize) -> Vec<super::words::title::Span> {
    use super::words::Text;
    let count = |key, count| Text::Plural(key, count, vec![]).spans(&m.i18n);
    if y == 0 { return count("tui.frame.selected", m.selected.len()); }
    if y == 1 { return layout::selection_bytes(m).map(|total| super::words::args("tui.frame.combined", vec![Text::Size(total)]).spans(&m.i18n)).unwrap_or_default(); }
    if y < 3 { return Vec::new(); }
    if y == m.selected_rows.len() + 3 && m.selected_rows.len() < m.selected.len() { return count("tui.frame.outside", m.selected.len() - m.selected_rows.len()); }
    Vec::new()
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
