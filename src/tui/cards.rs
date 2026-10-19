//! The window's cards in cells: a framed ground with two cells of air, the bold title beside a muted esc, the rule, and bracket buttons.
use crate::i18n::I18n;
use crate::jsondoc::Json;
use crate::tui::{cells::{fit, text_width, Wrapped}, input::Key, keymap::Map, model::Model, theme::Theme};
use crate::tui::wire::{count, flag, number, text, word, Wire};
use std::io;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};

// Interior air on each side, and the narrowest Properties card the window draws.
const AIR: usize = 2;
const PROPERTIES_WIDTH: usize = 44;

// Corners round only while Hyprland rounds, the compositor's own setting read when an overlay opens.
pub fn corners(m: &Model) -> [&'static str; 4] {
    if m.rounded.load(Ordering::Relaxed) { ["╭", "╮", "╰", "╯"] } else { ["┌", "┐", "└", "┘"] }
}
// Asks Hyprland for decoration:rounding off the input path; the next frame draws whatever it answered.
pub fn probe(rounded: &Arc<AtomicBool>) {
    if cfg!(test) || std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").map_or(true, |value| value.is_empty()) { return; }
    let rounded = Arc::clone(rounded);
    std::thread::spawn(move || {
        let mut command = std::process::Command::new("hyprctl");
        command.args(["-j", "getoption", "decoration:rounding"]);
        if let Some(answer) = crate::tui::icons::output(&mut command, &AtomicBool::new(false)) { rounded.store(rounding(&answer), Ordering::Relaxed); }
    });
}
// Sample input: {"option":"decoration:rounding","int":8,"set":true}; any radius above zero rounds.
fn rounding(answer: &str) -> bool {
    crate::jsondoc::parse(answer).ok().and_then(|value| value.get("int").and_then(Json::as_f64)).is_some_and(|radius| radius > 0.0)
}

pub struct Button { pub label: String, pub default: bool, pub focused: bool, pub enabled: bool }

// A card's cells, 0-based: the frame's corner, its interior width and rows, the body rows shown from top, and each button's x, y and width.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card { pub x: usize, pub y: usize, pub width: usize, pub height: usize, pub air: usize, pub top: usize, pub shown: usize, pub buttons: Vec<(usize, usize, usize)> }
impl Card {
    pub fn content(&self) -> usize { self.width.saturating_sub(2 * self.air) }
    pub fn left(&self) -> usize { self.x + 1 + self.air }
    // Whether a 1-based terminal cell falls on the card, frame included.
    pub fn contains(&self, x: usize, y: usize) -> bool {
        (self.x + 1..=self.x + self.width + 2).contains(&x) && (self.y + 1..=self.y + self.height + 2).contains(&y)
    }
    pub fn button(&self, x: usize, y: usize) -> Option<usize> {
        self.buttons.iter().position(|(bx, by, bw)| y == by + 1 && x > *bx && x <= bx + bw)
    }
}
// Two cells of air inside each side, dropped on a screen under 30 columns so the text keeps the room.
pub fn air(columns: usize) -> usize { if columns >= 30 { AIR } else { 0 } }
// The widest text a card can hold on this screen.
pub fn room(columns: usize) -> usize { columns.saturating_sub(2 + 2 * air(columns)).max(1) }
// Centred on the screen; buttons share the bottom row while they fit and stack otherwise, and a body taller than the screen scrolls.
pub fn layout(columns: usize, lines: usize, need: usize, body: usize, buttons: &[usize], scroll: usize) -> Card {
    let air = air(columns);
    let content = need.min(room(columns));
    let inline = buttons.iter().sum::<usize>() + 2 * buttons.len().saturating_sub(1) <= content;
    let button_rows = if buttons.is_empty() { 0 } else if inline { 1 } else { buttons.len() };
    let fixed = 2 + if button_rows > 0 { 1 + button_rows } else { 0 };
    let shown = body.min(lines.saturating_sub(2 + fixed));
    let (width, height) = (content + 2 * air, fixed + shown);
    let mut card = Card { x: columns.saturating_sub(width + 2) / 2, y: lines.saturating_sub(height + 2) / 2, width, height, air, top: scroll.min(body - shown), shown, buttons: Vec::new() };
    let (end, row) = (card.left() + content, card.y + 4 + shown);
    let mut right = end;
    for (index, &w) in buttons.iter().enumerate().rev() {
        if inline { card.buttons.insert(0, (right.saturating_sub(w), row, w)); right = right.saturating_sub(w + 2); }
        else { card.buttons.insert(0, (end.saturating_sub(w), row + index, w)); }
    }
    card
}
// The frame on the ground, the bold title with esc at the trailing end, and the rule under them.
pub fn frame(out: &mut String, m: &Model, theme: &Theme, card: &Card, title: &str) {
    let base = theme.base();
    let corners = corners(m);
    let at = |out: &mut String, y: usize| out.push_str(&format!("\x1b[{};{}H{}{}", y + 1, card.x + 1, base, theme.frame));
    at(out, card.y);
    out.push_str(&format!("{}{}{}", corners[0], "─".repeat(card.width), corners[1]));
    for y in card.y + 1..=card.y + card.height {
        at(out, y);
        out.push_str(&format!("│{}{}{}│", base, " ".repeat(card.width), theme.frame));
    }
    at(out, card.y + card.height + 1);
    out.push_str(&format!("{}{}{}{}", corners[2], "─".repeat(card.width), corners[3], base));
    let content = card.content();
    let esc = m.i18n.tr("card.esc");
    let room = content.saturating_sub(text_width(&esc) + 2);
    out.push_str(&format!("\x1b[{};{}H{}\x1b[1m{}{}", card.y + 2, card.left() + 1, base, fit(title, if room > 0 { room } else { content }), base));
    if room > 0 { out.push_str(&format!("  {}{}{}", theme.muted, esc, base)); }
    out.push_str(&format!("\x1b[{};{}H{}{}{}", card.y + 3, card.left() + 1, theme.rule, "─".repeat(content), base));
}
pub fn buttons(out: &mut String, theme: &Theme, card: &Card, buttons: &[Button]) {
    let base = theme.base();
    for ((x, y, w), button) in card.buttons.iter().zip(buttons) {
        let bracket = if button.focused { &theme.foreground } else if button.default { &theme.accent } else { &theme.frame };
        let fill = if button.default { theme.wash.as_str() } else { "" };
        let label = if button.enabled { &theme.foreground } else { &theme.muted };
        out.push_str(&format!("\x1b[{};{}H{}{}[{}{} {} {}{}]{}", y + 1, x + 1, base, bracket, fill, label, fit(&button.label, w.saturating_sub(4)), base, bracket, base));
    }
}
fn button_width(label: &str) -> usize { text_width(label) + 4 }

// Delete permanently: Cancel stays the default while Tab moves focus, and Enter or Space press the focused button.
fn delete_text(m: &Model) -> (String, Vec<String>, [Button; 2]) {
    let deletion = m.deletion.as_ref();
    let token = deletion.map_or(0, |d| d.token);
    let count = if token == 0 { m.selected.len().max(1) } else { deletion.map_or(1, |d| d.count) } as i64;
    let body = if token == 0 { m.i18n.tr("delete.inspecting") } else { m.i18n.trn("delete.body", count) };
    let focus = deletion.is_some_and(|d| d.destructive);
    let width = text_width(&body).min(room(m.columns));
    let rows = Wrapped::new(&body, width).map(str::to_owned).collect();
    (m.i18n.trn("delete.title", count), rows, [
        Button { label: m.i18n.tr("card.cancel"), default: true, focused: !focus, enabled: true },
        Button { label: m.i18n.tr("card.delete"), default: false, focused: focus, enabled: token != 0 },
    ])
}
pub fn delete_card(m: &Model) -> Card {
    let (title, rows, buttons) = delete_text(m);
    let widths: Vec<usize> = buttons.iter().map(|b| button_width(&b.label)).collect();
    let need = rows.iter().map(|row| text_width(row)).max().unwrap_or(0).max(text_width(&title) + 2 + text_width(&m.i18n.tr("card.esc")))
        .max(widths.iter().sum::<usize>() + 2);
    layout(m.columns, m.height + crate::tui::layout::CHROME_ROWS, need, rows.len(), &widths, m.deletion.as_ref().map_or(0, |d| d.scroll))
}
pub fn draw_delete(out: &mut String, m: &Model, theme: &Theme) {
    let (title, rows, labels) = delete_text(m);
    let card = delete_card(m);
    frame(out, m, theme, &card, &title);
    for (i, row) in rows.iter().skip(card.top).take(card.shown).enumerate() {
        out.push_str(&format!("\x1b[{};{}H{}{}", card.y + 4 + i, card.left() + 1, theme.base(), fit(row, card.content())));
    }
    buttons(out, theme, &card, &labels);
}
pub fn delete_key(m: &mut Model, key: &Key, wire: &mut Wire) -> io::Result<()> {
    let card = delete_card(m);
    let most = delete_text(m).1.len().saturating_sub(card.shown);
    let token = m.deletion.as_ref().map_or(0, |d| d.token);
    let mut activate = false;
    let mut cancel = key.name == "Escape";
    if let Some(pointer) = &key.pointer {
        if pointer.released { return Ok(()); }
        if matches!(pointer.button, 64 | 65) {
            let scroll = if pointer.button == 64 { card.top.saturating_sub(1) } else { (card.top + 1).min(most) };
            m.deletion.as_mut().unwrap().scroll = scroll;
        } else if !card.contains(pointer.x, pointer.y) && pointer.button == 0 && !pointer.motion { cancel = true; }
        else if let Some(index) = card.button(pointer.x, pointer.y).filter(|_| pointer.motion || pointer.button == 0) {
            if index == 1 && token == 0 { return Ok(()); }
            m.deletion.as_mut().unwrap().destructive = index == 1;
            activate = pointer.button == 0 && !pointer.motion;
        }
    } else {
        let deletion = m.deletion.as_mut().unwrap();
        if key.mods.is_empty() && key.name == "Up" { deletion.scroll = card.top.saturating_sub(1); }
        else if key.mods.is_empty() && key.name == "Down" { deletion.scroll = (card.top + 1).min(most); }
        else if (matches!(key.name.as_str(), "Tab" | "Backtab") && matches!(key.mods.as_str(), "" | "shift"))
            || (key.mods.is_empty() && (matches!(key.name.as_str(), "Right" | "Left") || matches!(key.text.as_str(), "h" | "l"))) {
            deletion.destructive = token != 0 && match key.name.as_str() {
                "Left" => false, "Right" => true,
                _ if key.text == "h" => false, _ if key.text == "l" => true,
                _ => !deletion.destructive,
            };
        } else if key.mods.is_empty() && matches!(key.name.as_str(), "Return" | "Enter" | "Space") { activate = true; }
    }
    let destructive = m.deletion.as_ref().is_some_and(|d| d.destructive);
    if cancel || (activate && !destructive) {
        m.deletion = None;
        m.menu_action.clear();
        wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(m.action_id))])?;
    } else if activate && token != 0 {
        m.deletion = None;
        m.menu_action = "deleteRunning".into();
        wire.send(vec![("c", word("menuaction")), ("op", word("delete")), ("id", number(m.action_id)), ("token", number(token))])?;
    }
    Ok(())
}

// Properties: the window's facts as a muted label column beside each value, closed by its default Close button.
pub fn properties(value: &Json, i18n: &I18n) -> Vec<(String, String)> {
    let fact = |key: &str, value: String| (i18n.tr(key), value);
    let bytes = count(value, "bytes");
    let mut facts = vec![
        fact("properties.path", text(value, "path").into()),
        fact("properties.kind", text(value, "kind").into()),
        fact("properties.size", i18n.trn_args("properties.bytes", bytes as i64, &[&crate::tui::render::bytes(bytes)])),
        fact("properties.permissions", text(value, "mode").into()),
        fact("properties.owner", i18n.tr_args("properties.ownerValue", &[text(value, "owner"), &count(value, "uid").to_string()])),
        fact("properties.group", i18n.tr_args("properties.groupValue", &[&count(value, "gid").to_string()])),
    ];
    let modified = crate::tui::calendar::value(value.get("modified").unwrap_or(&Json::Null));
    if !modified.is_empty() { facts.insert(3, fact("properties.modified", modified)); }
    let deleted = crate::tui::calendar::value(value.get("deleted").unwrap_or(&Json::Null));
    if !deleted.is_empty() { facts.push(fact("properties.deleted", deleted)); }
    if flag(value, "symlink") { facts.push(fact("properties.target", text(value, "target").into())); }
    facts
}
// Each fact's rows: its label beside the value's first line, continuation lines under the value column.
fn property_rows(m: &Model) -> (usize, Vec<(String, String)>) {
    let facts = m.properties.as_deref().unwrap_or_default();
    let label = facts.iter().map(|(label, _)| text_width(label)).max().unwrap_or(0);
    let label = if label == 0 { 0 } else { label + 2 };
    let longest = facts.iter().map(|(_, value)| label + text_width(value)).max().unwrap_or(0).max(PROPERTIES_WIDTH);
    let room = longest.min(room(m.columns)).saturating_sub(label).max(1);
    let rows = facts.iter().flat_map(|(name, value)| Wrapped::new(value, room).enumerate()
        .map(move |(i, line)| (if i == 0 { name.clone() } else { String::new() }, line.to_owned())).collect::<Vec<_>>()).collect();
    (label, rows)
}
pub fn properties_card(m: &Model) -> Card {
    let (label, rows) = property_rows(m);
    let close = button_width(&m.i18n.tr("card.close"));
    let need = rows.iter().map(|(_, value)| label + text_width(value)).max().unwrap_or(0).max(PROPERTIES_WIDTH)
        .max(text_width(&m.i18n.tr("properties.title")) + 2 + text_width(&m.i18n.tr("card.esc")));
    layout(m.columns, m.height + crate::tui::layout::CHROME_ROWS, need, rows.len(), &[close], m.sheet_top)
}
pub fn draw_properties(out: &mut String, m: &Model, theme: &Theme) {
    let card = properties_card(m);
    let (label, rows) = property_rows(m);
    frame(out, m, theme, &card, &m.i18n.tr("properties.title"));
    for (i, (name, value)) in rows.iter().skip(card.top).take(card.shown).enumerate() {
        let room = card.content().saturating_sub(label);
        out.push_str(&format!("\x1b[{};{}H{}{}{}{}{}", card.y + 4 + i, card.left() + 1, theme.base(), theme.muted, fit(name, label.min(card.content())), theme.foreground, fit(value, room)));
    }
    buttons(out, theme, &card, &[Button { label: m.i18n.tr("card.close"), default: true, focused: true, enabled: true }]);
}
// Keys for the Properties card and the keys sheet: Escape, Enter and ? close, the arrows scroll, a click outside closes.
pub fn sheet_key(m: &mut Model, key: &Key, map: &Map) -> io::Result<()> {
    let card = if m.properties.is_some() { properties_card(m) } else { super::key_sheet::card(m, map) };
    let limit = if m.properties.is_some() { property_rows(m).1.len().saturating_sub(card.shown) } else { super::key_sheet::limit(m, map) };
    let mut close = false;
    if let Some(pointer) = &key.pointer {
        if pointer.released { return Ok(()); }
        if matches!(pointer.button, 64 | 65) { m.sheet_top = if pointer.button == 64 { m.sheet_top.saturating_sub(1) } else { (m.sheet_top + 1).min(limit) }; }
        else if pointer.button == 0 && !pointer.motion { close = !card.contains(pointer.x, pointer.y) || card.button(pointer.x, pointer.y).is_some(); }
    } else {
        let action = map.in_context(key, &m.preset, "panel");
        if matches!(action.as_str(), "escape" | "open") || (key.mods.is_empty() && key.text == "?") { close = true; }
        else if action == "cursorDown" { m.sheet_top = (m.sheet_top + 1).min(limit); }
        else if action == "cursorUp" { m.sheet_top = m.sheet_top.saturating_sub(1); }
    }
    if close {
        m.sheet = false;
        m.properties = None;
    }
    Ok(())
}

#[cfg(test)]
#[path = "cards_tests.rs"]
pub(crate) mod tests;
#[cfg(test)]
#[path = "cards_sheet_tests.rs"]
mod sheet_tests;
