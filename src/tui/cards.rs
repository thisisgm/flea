//! The window's cards in cells: a framed ground with two cells of air, the bold title beside a muted esc, the rule, and bracket buttons.
use crate::i18n::I18n;
use crate::jsondoc::Json;
use crate::tui::{cells::{fit, name_title, text_width, Wrapped}, input::{Key, WHEEL_DOWN, WHEEL_UP}, keymap::Map, model::Model, theme::Theme};
use crate::tui::wire::{count, flag, number, text, word, Wire};
use std::io;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};

// Interior air on each side, and the narrowest Properties card the window draws.
const AIR: usize = 2;
const PROPERTIES_WIDTH: usize = 44;

#[path = "cards/words.rs"]
pub(crate) mod words;

pub fn rtl(m: &Model) -> bool { m.i18n.locale().right_to_left && crate::tui::cells::mirrors("panels") }
pub fn column(m: &Model, card: &Card, offset: usize, width: usize) -> usize {
    card.left() + if rtl(m) { card.content().saturating_sub(offset + width) } else { offset }
}
pub fn direction(m: &Model, mut card: Card) -> Card {
    if rtl(m) {
        let end = card.left() + card.content();
        let left = card.left();
        for (x, _, width) in &mut card.buttons { *x = end.saturating_sub(*x - left + *width); }
    }
    card
}
pub fn field_text(m: &Model, text: &str, width: usize) -> String {
    let text = crate::tui::cells::clean(text);
    if rtl(m) {
        // ellipsis pads a short text on the right through fit, which would leave nothing to right-align.
        let shown = if text_width(&text) <= width { text } else { crate::tui::cells::ellipsis(&text, width) };
        format!("{}{}", " ".repeat(width.saturating_sub(text_width(&shown))), crate::tui::cells::visual::run(&shown))
    } else { fit(&text, width) }
}

// The event loop supplies one locale identity before card projection and hit testing.
pub fn refresh_locale(m: &mut Model) {
    crate::tui::print::refresh_locale(m);
    crate::tui::convert::refresh_locale(m);
    crate::tui::combine::refresh_locale(m);
    crate::tui::jobstatus::refresh_locale(m);
    m.taildrop.refresh_locale(&m.i18n);
    if let Some((id, source)) = &m.menu_view.properties_source {
        if *id == m.action_id && m.properties.as_ref().is_some_and(|facts| !facts.is_empty()) {
            m.properties = Some(properties(source, &m.i18n));
        }
    }
}

// Corners round only while Hyprland rounds, the compositor's own setting read when an overlay opens.
pub fn corners(m: &Model) -> [&'static str; 4] {
    if m.rounded.load(Ordering::Relaxed) { ["╭", "╮", "╰", "╯"] } else { ["┌", "┐", "└", "┘"] }
}
// Asks Hyprland for decoration:rounding off the input path; the next frame draws whatever it answered.
pub fn probe(rounded: &Arc<AtomicBool>) {
    if cfg!(test) { return; }
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none_or(|value| value.is_empty()) {
        rounded.store(false, Ordering::Relaxed);
        return;
    }
    let rounded = Arc::clone(rounded);
    std::thread::spawn(move || {
        let mut command = std::process::Command::new("hyprctl");
        command.args(["-j", "getoption", "decoration:rounding"]);
        rounded.store(read_rounding(&mut command), Ordering::Relaxed);
    });
}
fn read_rounding(command: &mut std::process::Command) -> bool {
    crate::tui::icons::output(command, &AtomicBool::new(false)).as_deref().is_some_and(rounding)
}
// Sample input: {"option":"decoration:rounding","int":8,"set":true}; any radius above zero rounds.
fn rounding(answer: &str) -> bool {
    crate::jsondoc::parse(answer).ok()
        .filter(|value| value.get("set").and_then(Json::as_bool) != Some(false))
        .and_then(|value| value.get("int").and_then(Json::as_f64))
        .is_some_and(|radius| radius.is_finite() && radius > 0.0)
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
        let w = w.min(content);
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
    let title_width = if room > 0 { room } else { content };
    let title_x = column(m, card, 0, title_width);
    let padding = if rtl(m) { title_width.saturating_sub(text_width(title)) } else { 0 };
    out.push_str(&format!("\x1b[{};{}H{}{}{}{}", card.y + 2, title_x + 1, base, " ".repeat(padding), name_title("", title, title_width - padding), base));
    if room > 0 {
        let x = column(m, card, title_width + 2, text_width(&esc));
        out.push_str(&format!("\x1b[{};{}H{}{}{}", card.y + 2, x + 1, theme.muted, esc, base));
    }
    out.push_str(&format!("\x1b[{};{}H{}{}{}", card.y + 3, card.left() + 1, theme.rule, "─".repeat(content), base));
}
pub fn buttons(out: &mut String, theme: &Theme, card: &Card, buttons: &[Button]) {
    let base = theme.base();
    for ((x, y, w), button) in card.buttons.iter().zip(buttons) {
        let bracket = if button.focused { &theme.foreground } else if button.default { &theme.accent } else { &theme.frame };
        let fill = if button.default { theme.wash.as_str() } else { "" };
        let label = if button.enabled { &theme.foreground } else { &theme.muted };
        let attribute =
            theme
                .depth
                .control_attribute(button.enabled, button.focused, button.default);
        out.push_str(&format!("\x1b[{};{}H{}{attribute}{}[{}{} {} {}{attribute}{}]{}", y + 1, x + 1, base, bracket, fill, label, fit(&button.label, w.saturating_sub(4)), base, bracket, base));
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
    direction(m, layout(m.columns, m.height + crate::tui::layout::CHROME_ROWS, need, rows.len(), &widths, m.deletion.as_ref().map_or(0, |d| d.scroll)))
}
pub fn draw_delete(out: &mut String, m: &Model, theme: &Theme) {
    let (title, rows, labels) = delete_text(m);
    let card = delete_card(m);
    frame(out, m, theme, &card, &title);
    for (i, row) in rows.iter().skip(card.top).take(card.shown).enumerate() {
        out.push_str(&format!("\x1b[{};{}H{}{}", card.y + 4 + i, card.left() + 1, theme.base(), field_text(m, row, card.content())));
    }
    buttons(out, theme, &card, &labels);
}
pub fn delete_key(m: &mut Model, key: &Key, wire: &mut Wire) -> io::Result<()> {
    let card = delete_card(m);
    let rtl = rtl(m) && crate::tui::cells::mirrors("navigation");
    let most = delete_text(m).1.len().saturating_sub(card.shown);
    let token = m.deletion.as_ref().map_or(0, |d| d.token);
    let mut activate = false;
    let mut cancel = key.name == "Escape";
    if let Some(pointer) = &key.pointer {
        if pointer.released { return Ok(()); }
        if matches!(pointer.button, WHEEL_UP | WHEEL_DOWN) {
            let scroll = if pointer.button == WHEEL_UP { card.top.saturating_sub(1) } else { (card.top + 1).min(most) };
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
                "Left" => rtl, "Right" => !rtl,
                _ if key.text == "h" => rtl, _ if key.text == "l" => !rtl,
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
pub fn properties_reply(m: &mut Model, value: &Json) {
    let mime_kind = m
        .rows
        .values()
        .chain(m.selected_rows.values())
        .find(|row| {
            m.path.join(flea::bytename::text_path(&row.name)).as_path()
                == &*flea::bytename::text_path(text(value, "path"))
        })
        .map(|row| row.kind.clone())
        .unwrap_or_default();
    let mut value = value.clone();
    if value.get("mimeKind").is_none() && !mime_kind.is_empty() {
        if let Json::Obj(fields) = &mut value {
            fields.push(("mimeKind".into(), word(&mime_kind)));
        }
    }
    m.menu_view.properties_source = Some((m.action_id, value.clone()));
    m.properties = Some(properties(&value, &m.i18n));
}
pub fn properties(value: &Json, i18n: &I18n) -> Vec<(String, String)> {
    let fact = |key: &str, value: String| (i18n.tr(key), value);
    let bytes = count(value, "bytes");
    let kind = match text(value, "kind") {
        "File" | "Directory" | "Symbolic link" | "Special file" => flea::ui_logic_open_with_properties::properties::kind_word(
            i18n, &flea::ui_logic_open_with_properties::properties::Facts {
                kind: text(value, "kind").into(), directory: flag(value, "directory"), ..Default::default()
            }, text(value, "mimeKind")),
        description => description.into(),
    };
    let mut facts = vec![
        fact("properties.path", text(value, "path").into()),
        fact("properties.kind", kind),
        fact("properties.size", i18n.trn_args("properties.bytes", bytes as i64, &[&i18n.locale().format_size(bytes as u64)])),
        fact("properties.permissions", text(value, "mode").into()),
        fact("properties.owner", i18n.tr_args("properties.ownerValue", &[text(value, "owner"), &count(value, "uid").to_string()])),
        fact("properties.group", i18n.tr_args("properties.groupValue", &[&count(value, "gid").to_string()])),
    ];
    let modified = crate::tui::calendar::localized_value(value.get("modified").unwrap_or(&Json::Null), i18n.locale());
    if !modified.is_empty() { facts.insert(3, fact("properties.modified", modified)); }
    if value.get("birth").is_some() {
        let birth = crate::tui::calendar::localized_value(value.get("birth").unwrap(), i18n.locale());
        facts.insert(3, fact("properties.birth", if birth.is_empty() { i18n.tr("pane.unavailable") } else { birth }));
    }
    let deleted = crate::tui::calendar::localized_value(value.get("deleted").unwrap_or(&Json::Null), i18n.locale());
    if !deleted.is_empty() { facts.push(fact("properties.deleted", deleted)); }
    if flag(value, "symlink") { facts.push(fact("properties.target", text(value, "target").into())); }
    facts
}
// The label column takes at most half the room, so a long label leaves the value the other half.
fn label_column(columns: usize, label: usize) -> usize { label.min(room(columns) / 2) }
// Each fact's rows: its label beside the value's first line, continuation lines under the value column.
fn property_rows(m: &Model) -> (usize, Vec<(String, String)>) {
    let facts = m.properties.as_deref().unwrap_or_default();
    let label = facts.iter().map(|(label, _)| text_width(label)).max().unwrap_or(0);
    let label = if label == 0 { 0 } else { label + 2 };
    let label = label_column(m.columns, label);
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
    direction(m, layout(m.columns, m.height + crate::tui::layout::CHROME_ROWS, need, rows.len(), &[close], m.sheet_top))
}
pub fn draw_properties(out: &mut String, m: &Model, theme: &Theme) {
    let card = properties_card(m);
    let (label, rows) = property_rows(m);
    frame(out, m, theme, &card, &m.i18n.tr("properties.title"));
    for (i, (name, value)) in rows.iter().skip(card.top).take(card.shown).enumerate() {
        let room = card.content().saturating_sub(label);
        let name_x = column(m, &card, 0, label.min(card.content()));
        let value_x = column(m, &card, label, room);
        out.push_str(&format!("\x1b[{};{}H{}{}{}\x1b[{};{}H{}{}", card.y + 4 + i, name_x + 1, theme.base(), theme.muted, field_text(m, name, label.min(card.content())), card.y + 4 + i, value_x + 1, theme.foreground, crate::tui::cells::isolate(crate::tui::cells::Run::Name, &field_text(m, value, room))));
    }
    buttons(out, theme, &card, &[Button { label: m.i18n.tr("card.close"), default: true, focused: true, enabled: true }]);
}
// Keys for the Properties card and the keys sheet: Escape, Enter, ? and q close, the listing's motion keys scroll, a click outside closes.
pub fn sheet_key(m: &mut Model, key: &Key, map: &Map, armed: &str) -> io::Result<()> {
    let card = if m.properties.is_some() { properties_card(m) } else { super::key_sheet::card(m, map) };
    let limit = if m.properties.is_some() { property_rows(m).1.len().saturating_sub(card.shown) } else { super::key_sheet::limit(m, map) };
    let mut close = false;
    if let Some(pointer) = &key.pointer {
        if pointer.released { return Ok(()); }
        if matches!(pointer.button, WHEEL_UP | WHEEL_DOWN) { m.sheet_top = if pointer.button == WHEEL_UP { m.sheet_top.saturating_sub(1) } else { (m.sheet_top + 1).min(limit) }; }
        else if pointer.button == 0 && !pointer.motion { close = !card.contains(pointer.x, pointer.y) || card.button(pointer.x, pointer.y).is_some(); }
    } else {
        let panel = map.in_context(key, &m.preset, "panel");
        let action = crate::tui::preview_keys::listing_action(m, key, map, armed, &panel);
        let page = card.shown.max(1);
        // The panel's own Escape and Enter close it, and so does the listing's quit key; a listing key that opens or moves elsewhere is consumed.
        if matches!(panel.as_str(), "escape" | "open")
            || matches!(action.as_str(), "quit" | "quitNoCwd")
            || (key.mods.is_empty() && key.text == "?")
        {
            close = true;
        } else if action == "prefix:g" || action == "cursorFirstArm" && armed != action {
            m.key_arm = action;
        } else {
            m.sheet_top = match action.as_str() {
                "cursorDown" => m.sheet_top + 1,
                "cursorUp" => m.sheet_top.saturating_sub(1),
                "pageDown" => m.sheet_top + page,
                "pageUp" => m.sheet_top.saturating_sub(page),
                "halfPageDown" => m.sheet_top + (page / 2).max(1),
                "halfPageUp" => m.sheet_top.saturating_sub((page / 2).max(1)),
                "first" | "cursorFirst" | "cursorFirstArm" => 0,
                "last" | "cursorLast" => limit,
                _ => m.sheet_top,
            }
            .min(limit);
        }
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
#[cfg(test)]
#[path = "cards_corner_tests.rs"]
mod corner_tests;

impl Model {
    /// A catalog string as a heading in capitals, cased the way the active language writes them (Turkish dotted I).
    pub fn heading(&self, key: &str) -> String {
        flea::ui_logic_settings_pages::caps::heading(&self.i18n.tr(key), self.i18n.language())
    }
}
#[cfg(test)]
#[path = "cards_words_tests.rs"]
pub(crate) mod words_tests;
