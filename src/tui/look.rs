use super::cells::{ellipsis, fit, name_title, text_width};
use super::layout::{BODY_ROW, CHROME_ROWS};
use super::model::Model;
use super::pane::Area;
use super::pane_state::{self, Kind};
use super::theme::Theme;
use std::time::Duration;

// Quick Look sits nine cells in from each side, down to a card this many cells wide.
const MARGIN: usize = 9;
const NARROWEST: usize = 24;
// The bar, the rule and one blank row sit above the content, and the frame takes a row at each end.
const ABOVE: usize = 4;

fn outer(columns: usize) -> (usize, usize) {
    let margin = MARGIN.min(columns.saturating_sub(NARROWEST) / 2);
    (margin, columns.saturating_sub(margin * 2))
}
// The card's first column inside its frame, zero-based.
pub fn left(columns: usize) -> usize {
    outer(columns).0 + 1
}
// The card's content area inside its frame, with two cells of air at each side.
pub fn area(columns: usize, height: usize) -> Area {
    let interior = outer(columns).1.saturating_sub(2);
    Area { columns: interior, padding: if interior >= 10 { 2 } else { 0 }, height: text_rows(height), card: true }
}
pub fn text_rows(height: usize) -> usize {
    height.saturating_sub(ABOVE + 1)
}

// The screen while Quick Look is open: everything behind it mixed halfway to the ground, then the card.
pub fn frame(m: &Model, theme: &Theme, map: &super::keymap::Map, columns: usize, lines: usize, elapsed: Duration) -> String {
    if !m.quicklook || columns < 12 || lines <= CHROME_ROWS {
        return super::render::frame(m, theme, map, columns, lines, elapsed);
    }
    let mut out = super::render::frame(m, theme.dim.as_deref().unwrap_or(theme), map, columns, lines, elapsed);
    out.push_str(&card(m, theme, columns));
    out
}

pub fn card(m: &Model, theme: &Theme, columns: usize) -> String {
    let (x, width) = outer(columns);
    let area = area(columns, m.height);
    let corners = super::render::cards::corners(m);
    let base = theme.base();
    let edge = |row: usize, left: &str, right: &str| {
        format!("\x1b[{row};{}H{base}{}{left}{}{right}{base}", x + 1, theme.frame, "─".repeat(width.saturating_sub(2)))
    };
    let mut out = edge(BODY_ROW, corners[0], corners[1]);
    let pad = " ".repeat(area.padding);
    for r in 1..m.height.saturating_sub(1) {
        let inner = match r {
            1 => format!("{base}{pad}{}{base}{pad}", bar(m, area.width(), theme)),
            2 => format!("{base}{pad}{}{}{base}{pad}", theme.rule, "─".repeat(area.width())),
            3 => format!("{base}{}", " ".repeat(area.columns)),
            _ => super::pane::body(m, r - ABOVE, &area, theme),
        };
        out.push_str(&format!("\x1b[{};{}H{base}{}│{inner}{base}{}│{base}", BODY_ROW + r, x + 1, theme.frame, theme.frame));
    }
    out.push_str(&edge(BODY_ROW + m.height.saturating_sub(1), corners[2], corners[3]));
    out
}

// The bar: the mark and bold name, the file's one fact in meta, and the close mark two cells from the end.
fn bar(m: &Model, width: usize, theme: &Theme) -> String {
    let Some(row) = m.rows.get(&m.cursor) else { return fit("", width) };
    let close = if m.marks() { "\u{ea76}" } else { "×" };
    let room = width.saturating_sub(3);
    let mark = if m.marks() { format!("{} ", super::icons::row(row, true)) } else { String::new() };
    let leaf = row.name.rsplit('/').next().unwrap_or(&row.name);
    let plates = if pane_state::kind(row) == Kind::Model3d { plate_words(m, room / 2, theme) } else { (String::new(), 0) };
    let room = room.saturating_sub(plates.1);
    let head = (text_width(&mark) + text_width(leaf)).min(room);
    let name = name_title(&mark, leaf, head);
    let fact = fact(m, row);
    let rest = room - head;
    let fact = if fact.is_empty() || rest < 3 { fit("", rest) } else { format!("  {}", ellipsis(&fact, text_width(&fact).min(rest - 2))) };
    let gap = width.saturating_sub(2 + head + text_width(&fact) + plates.1);
    format!("{}{name}{}{fact}{}{}{}{close} ", theme.foreground, theme.meta, " ".repeat(gap), plates.0, theme.foreground)
}

fn plate_words(m: &Model, width: usize, theme: &Theme) -> (String, usize) {
    let count = m.model3d.data.as_ref().map_or(0, |d| d.plates.len());
    if count < 2 { return (String::new(), 0); }
    let mut text = String::new();
    let mut used = 0;
    for plate in 0..count {
        let number = m.preview_state.i18n.locale().format_count((plate + 1) as i64);
        let word = m.preview_state.i18n.tr_args("model3d.plate", &[&number]);
        let length = text_width(&word) + 2;
        if used + length > width { break; }
        text.push_str(if plate == m.model3d.controls.plate { &theme.foreground } else { &theme.muted });
        text.push_str(&word);
        text.push_str("  ");
        used += length;
    }
    (text, used)
}

// The fact the window's Quick Look bar names: a picture's size, a sound's length, a page or line count.
fn fact(m: &Model, row: &super::model::Row) -> String {
    let i18n = &m.preview_state.i18n;
    let probe = m.media_probe.unwrap_or((0, 0, 0));
    match pane_state::kind(row) {
        Kind::Model3d => super::model3d::dimensions(m).unwrap_or_default(),
        Kind::Image | Kind::Video if probe.0 > 0 && probe.1 > 0 => format!("{} × {}", probe.0, probe.1),
        Kind::Audio if probe.2 > 0 => super::media::clock(probe.2 as f64 / 1000.0),
        Kind::Pdf => m.pdf.as_ref().filter(|pdf| pdf.pages > 0).map(|pdf| i18n.trn("quicklook.pages", pdf.pages as i64)).unwrap_or_default(),
        Kind::Archive => m.preview_state.archive.as_ref().map(|a| i18n.trn("preview.entries", a.entries as i64)).unwrap_or_default(),
        _ if !m.preview_body.is_empty() => i18n.trn("quicklook.lines", m.preview_body.lines().count() as i64),
        _ => String::new(),
    }
}

// A press on the close mark or outside the card closes Quick Look and gives the keyboard back to the listing.
pub fn press(m: &mut Model, pointer: &super::input::Pointer) -> bool {
    let closing = m.quicklook && pointer.button == 0 && !pointer.motion && !pointer.released && closes(m, pointer.x, pointer.y);
    if closing {
        (m.quicklook, m.preview_focus) = (false, false);
    }
    closing
}

// Whether a press at a one-based cell closes the card: on its close mark or anywhere outside it.
fn closes(m: &Model, x: usize, y: usize) -> bool {
    let (left, width) = outer(m.columns);
    let area = area(m.columns, m.height);
    let close = (BODY_ROW + 1, left + 2 + area.padding + area.width() - 2);
    let inside = x > left && x <= left + width && y >= BODY_ROW && y < BODY_ROW + m.height;
    !inside || (y, x) == close
}

#[cfg(test)]
#[path = "look_tests.rs"]
mod tests;
