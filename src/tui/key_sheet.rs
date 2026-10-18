//! The keys sheet: a card inside the chrome rules holding the window's groups, each whole, in three columns, each key as this preset resolves it.
use super::cards::{self, Card};
use crate::tui::{cells::{fit, text_width, Wrapped}, input::Key, keymap::Map, model::Model, theme::Theme};

// Each row's group, the actions it answers for, its words, and the spellings the window's sheet prints, a kitty-protocol key before the key that works without it.
type Row = (&'static str, &'static [&'static str], &'static str, &'static [&'static str]);
const ROWS: &[Row] = &[
    ("move", &["cursorDown"], "keys.cursorDown", &["j", "down"]),
    ("move", &["cursorUp"], "keys.cursorUp", &["k", "up"]),
    ("move", &["cursorFirst"], "keys.cursorFirst", &["g", "home"]),
    ("move", &["cursorLast"], "keys.cursorLast", &["G", "end"]),
    ("move", &["pageDown"], "keys.pageDown", &["ctrl-d"]),
    ("move", &["pageUp"], "keys.pageUp", &["ctrl-u"]),
    ("move", &["open"], "keys.open", &["enter"]),
    ("move", &["parent"], "keys.parent", &["h", "left"]),
    ("move", &["historyBack"], "keys.historyBack", &["H"]),
    ("move", &["historyForward"], "keys.historyForward", &["L"]),
    ("move", &["focusNext", "focusPreview"], "keys.focusPreview", &["tab", "ctrl-tab"]),
    ("move", &["tabNew"], "keys.tabNew", &["t"]),
    ("move", &["openTab"], "keys.openTab", &["ctrl-enter", "O"]),
    ("move", &["tabClose"], "keys.tabClose", &["w", "ctrl-w"]),
    ("move", &["tabNext"], "keys.tabNext", &["ctrl-pagedown"]),
    ("move", &["tabPrevious"], "keys.tabPrevious", &["ctrl-pageup"]),
    ("move", &["tabMoveLeft"], "keys.tabMoveLeft", &["{"]),
    ("move", &["tabMoveRight"], "keys.tabMoveRight", &["}"]),
    ("move", &["escape"], "keys.escape", &["escape"]),
    ("move", &["tab1"], "keys.selectTab", &["1"]),
    ("move", &["quit"], "keys.quit", &["q"]),
    ("look", &["keymapSheet"], "keys.keymapSheet", &["?"]),
    ("look", &["preview"], "keys.preview", &["space"]),
    ("look", &["toggleHidden"], "keys.toggleHidden", &["."]),
    ("look", &["detailNext"], "keys.detailNext", &["I"]),
    ("find", &["filter"], "keys.filter", &["/"]),
    ("find", &["search"], "keys.search", &["f", "ctrl-f"]),
    ("find", &["searchContents"], "keys.searchContents", &["ctrl-g"]),
    ("find", &["reveal"], "keys.reveal", &["o"]),
    ("find", &["sortNext"], "keys.sortNext", &["s"]),
    ("find", &["sortReverse"], "keys.sortReverse", &["S"]),
    ("find", &["pathBar"], "keys.pathBar", &[":", "ctrl-l"]),
    ("change", &["copy"], "keys.copy", &["y", "ctrl-c"]),
    ("change", &["cut"], "keys.cut", &["x", "ctrl-x"]),
    ("change", &["paste"], "keys.paste", &["p", "ctrl-v"]),
    ("change", &["rename"], "keys.rename", &["r", "f2"]),
    ("change", &["trash"], "keys.trash", &["dd", "delete"]),
    ("change", &["deletePermanently"], "keys.deletePermanently", &["shift-delete"]),
    ("change", &["undo"], "keys.undo", &["z", "ctrl-z"]),
    ("change", &["redo"], "keys.redo", &["ctrl-shift-z", "ctrl-y"]),
    ("change", &["newFolder"], "keys.newFolder", &["ctrl-shift-n", "N"]),
    ("change", &["toggleSelect"], "keys.toggleSelect", &["v"]),
    ("change", &["menu"], "keys.menu", &["m", "menu"]),
    ("change", &["openTerminal"], "keys.openTerminal", &["ctrl-t"]),
    ("change", &["openAgent"], "keys.openAgent", &["A"]),
    ("change", &["print"], "keys.print", &["ctrl-p"]),
];
const GROUPS: [&str; 4] = ["move", "look", "find", "change"];
// At most three columns, four cells apart.
const COLUMNS: usize = 3;
const GAP: usize = 4;

// Sample input: "ctrl-shift-z", "shift-delete", "G", "space" or "enter", the window's key spellings.
fn keys(spelled: &str) -> Vec<Key> {
    let (mods, name) = [("ctrl-shift-", "ctrlshift"), ("ctrl-", "ctrl"), ("shift-", "shift"), ("alt-", "alt")].iter()
        .find_map(|(prefix, mods)| spelled.strip_prefix(prefix).map(|rest| (*mods, rest))).unwrap_or(("", spelled));
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if mods.is_empty() => vec![Key::character(c, "")],
        (Some(c), None) => vec![Key::named(&c.to_uppercase().to_string(), mods)],
        _ if name == "space" => vec![Key::character(' ', mods)],
        _ if name == "enter" => vec![Key::named("Return", mods), Key::named("Enter", mods)],
        _ => {
            let named = match name { "pagedown" => "PageDown".into(), "pageup" => "PageUp".into(), other => other[..1].to_uppercase() + &other[1..] };
            vec![Key::named(&named, mods)]
        }
    }
}
// The spelling as this preset binds it to one of the row's actions: a letter that arms its action prints doubled, "dd" prints "d" where one press acts.
fn bound(map: &Map, preset: &str, actions: &[&str], spelled: &str) -> Option<String> {
    let first = spelled.chars().next()?;
    let doubled = spelled.chars().count() == 2 && spelled.chars().all(|c| c == first);
    let probe = if doubled { first.to_string() } else { spelled.to_owned() };
    keys(&probe).iter().find_map(|key| {
        let found = map.action(key, preset);
        actions.iter().find_map(|action| if found == *action { Some(probe.clone()) }
            else if found == format!("{action}Arm") { Some(format!("{first}{first}")) } else { None })
    })
}
// The keymap's own spelling, "Ctrl+Shift+N" or "Return", in the sheet's lowercase form.
fn lowercase(chord: &str) -> String {
    let (mods, name) = chord.rsplit_once('+').filter(|(_, name)| !name.is_empty()).unwrap_or(("", chord));
    let name = if mods.is_empty() && name.chars().count() == 1 { name.to_owned() } else { name.to_lowercase().replace("return", "enter") };
    if mods.is_empty() { name } else { format!("{}-{}", mods.to_lowercase().replace('+', "-"), name) }
}
// The keys a row prints: the window's spellings this preset binds, else the preset's own first key for the row.
fn spelling(map: &Map, preset: &str, row: &Row) -> String {
    if row.1 == ["tab1"] { return if bound(map, preset, row.1, "1").is_some() { "1–9".into() } else { String::new() }; }
    let mut keys: Vec<String> = Vec::new();
    for spelled in row.3.iter().filter_map(|spelled| bound(map, preset, row.1, spelled)) {
        if !keys.contains(&spelled) { keys.push(spelled); }
    }
    if keys.is_empty() { keys.extend(row.1.iter().find_map(|action| map.chord(action, preset, "listing")).map(|chord| lowercase(&chord))); }
    keys.join(" / ")
}
// A menu row's key hint: the first key the row's action answers to without Ctrl or Alt.
pub fn hint(map: &Map, preset: &str, action: &str) -> String {
    ROWS.iter().find(|row| row.1.contains(&action)).and_then(|row| row.3.iter()
        .filter(|spelled| !spelled.starts_with("ctrl-") && !spelled.starts_with("alt-"))
        .find_map(|spelled| bound(map, preset, row.1, spelled))).unwrap_or_default()
}
// A sheet line: a group heading, a key beside its words, or, on a screen too narrow for that, a key or its words on a line of their own.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Line { Heading(String), Binding(String, String), Key(String), Words(String) }
fn groups(m: &Model, map: &Map) -> Vec<Vec<Line>> {
    GROUPS.iter().map(|group| {
        let mut lines = vec![Line::Heading(m.i18n.tr(&format!("keys.group.{group}")))];
        let answered = |row: &&Row| row.1.iter().any(|action| *action == "tab1" || crate::tui::keymap::supported(action));
        lines.extend(ROWS.iter().filter(|row| row.0 == *group).filter(answered).filter_map(|row| {
            let keys = spelling(map, &m.preset, row);
            (!keys.is_empty()).then(|| Line::Binding(keys, m.i18n.tr(row.2)))
        }));
        lines
    }).filter(|lines| lines.len() > 1).collect()
}
// Groups fill a column in order and the next group starts a new one when it would not fit whole.
fn pack(groups: &[Vec<Line>], capacity: usize) -> Vec<Vec<Line>> {
    let mut columns: Vec<Vec<Line>> = Vec::new();
    for group in groups {
        if columns.last().map_or(true, |column| !column.is_empty() && column.len() + group.len() > capacity) { columns.push(Vec::new()); }
        columns.last_mut().unwrap().extend(group.iter().cloned());
    }
    columns
}
// A column's key width and label width; a heading or a line of its own wider than both spills into the label width.
fn widths(column: &[Line]) -> (usize, usize) {
    let key = column.iter().map(|line| if let Line::Binding(keys, _) = line { text_width(keys) } else { 0 }).max().unwrap_or(0);
    let label = column.iter().map(|line| match line {
        Line::Binding(_, words) => text_width(words),
        Line::Heading(text) | Line::Key(text) | Line::Words(text) => text_width(text).saturating_sub(key + 2),
    }).max().unwrap_or(0);
    (key, label)
}
fn total(columns: &[Vec<Line>]) -> usize {
    columns.iter().map(|column| { let (key, label) = widths(column); key + 2 + label }).sum::<usize>() + GAP * columns.len().saturating_sub(1)
}
// The columns and the rows the card shows; the last row inside the frame stays blank whenever two rows fit.
struct Sheet { columns: Vec<Vec<Line>>, rows: usize, height: usize }
// The most columns, up to three, that fit the screen with the fewest rows; a screen too narrow for one column of keys beside their words wraps each on lines of its own.
fn sheet(m: &Model, map: &Map) -> Sheet {
    let groups = groups(m, map);
    let height = m.height.saturating_sub(2).max(1);
    let rows = if height > 4 { height - 3 } else { height.saturating_sub(2) }.max(1);
    let room = cards::room(m.columns);
    for count in (1..=COLUMNS).rev() {
        let mut capacity = rows;
        while pack(&groups, capacity).len() > count { capacity += 1; }
        let columns = pack(&groups, capacity);
        if total(&columns) <= room { return Sheet { columns, rows, height }; }
    }
    let wrap = |text: &str| Wrapped::new(text, room).map(str::to_owned).collect::<Vec<_>>();
    let stacked = groups.iter().flatten().flat_map(|line| match line {
        Line::Binding(keys, words) => wrap(keys).into_iter().map(Line::Key).chain(wrap(words).into_iter().map(Line::Words)).collect(),
        Line::Heading(text) => wrap(text).into_iter().map(Line::Heading).collect(),
        other => vec![other.clone()],
    }).collect();
    Sheet { columns: vec![stacked], rows, height }
}
pub fn card(m: &Model, map: &Map) -> Card {
    let sheet = sheet(m, map);
    let need = total(&sheet.columns).max(text_width(&m.i18n.tr("keys.title")) + 2 + text_width(&m.i18n.tr("card.esc")));
    let air = cards::air(m.columns);
    let width = need.min(cards::room(m.columns)) + 2 * air;
    Card { x: m.columns.saturating_sub(width + 2) / 2, y: crate::tui::layout::BODY_ROW - 1, width, height: sheet.height,
        air, top: m.sheet_top.min(limit(m, map)), shown: sheet.rows, buttons: Vec::new() }
}
// How far the sheet scrolls when its columns are taller than the card.
pub fn limit(m: &Model, map: &Map) -> usize {
    let sheet = sheet(m, map);
    sheet.columns.iter().map(Vec::len).max().unwrap_or(0).saturating_sub(sheet.rows)
}
pub fn draw(out: &mut String, m: &Model, theme: &Theme, map: &Map) {
    let sheet = sheet(m, map);
    let card = card(m, map);
    cards::frame(out, m, theme, &card, &m.i18n.tr("keys.title"));
    let base = theme.base();
    let (mut x, end) = (card.left(), card.left() + card.content());
    for column in &sheet.columns {
        let (key, label) = widths(column);
        let room = end.saturating_sub(x);
        for (i, line) in column.iter().skip(card.top).take(sheet.rows).enumerate() {
            out.push_str(&format!("\x1b[{};{}H{}", card.y + 4 + i, x + 1, base));
            match line {
                Line::Heading(text) | Line::Key(text) => out.push_str(&format!("{}{}", theme.meta, fit(text, room.min(key + 2 + label)))),
                Line::Words(text) => out.push_str(&format!("{}{}", theme.foreground, fit(text, room.min(key + 2 + label)))),
                Line::Binding(keys, words) => out.push_str(&format!("{}{}  {}{}", theme.meta, fit(&format!("{}{keys}", " ".repeat(key.saturating_sub(text_width(keys)))), key.min(room)),
                    theme.foreground, fit(words, room.saturating_sub(key + 2).min(label)))),
            }
        }
        x += key + 2 + label + GAP;
        if x >= end { break; }
    }
    out.push_str(&base);
}

#[cfg(test)]
#[path = "key_sheet_tests.rs"]
mod tests;
