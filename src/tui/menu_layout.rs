//! The file menu in cells: which rows it has, where it sits under its row, what a cell hits and what a row does.
use crate::tui::{cells::{fit, text_width}, keymap::Map, model::Model, theme::Theme, wire::flag};
use crate::jsondoc::Json;
use std::path::PathBuf;

// The window's menu marks: Codicons for the verbs, each brand row keeping its own mark.
const OPEN: &str = "\u{eaf7}";
const WINDOW: &str = "\u{eb7f}";
const PRINTER: &str = "\u{f1786}";
const ARCHIVE: &str = "\u{ea98}";
const HASH: &str = "\u{ea90}";
const TAILSCALE: &str = "\u{f15fc}";
const DROPBOX: &str = "\u{f01e3}";
const TRASH: &str = "\u{ea81}";
const EYE: &str = "\u{ea70}";
const EYE_CLOSED: &str = "\u{eae7}";
const MACHINE: &str = "\u{ea7a}";
const FILE: &str = "\u{ea7b}";
const CHEVRON: &str = "\u{eab6}";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sub { #[default] Root, OpenWith, Compress, Taildrop }

// Plain and destructive rows act; a dim row waits for its facts and an unavailable one says why it cannot act.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ink { Plain, Dim, Destructive, Unavailable }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry { pub id: String, pub mark: &'static str, pub label: String, pub trail: String, pub keycap: bool, pub chevron: bool, pub ink: Ink }
impl Entry {
    fn new(id: &str, mark: &'static str, label: String, ink: Ink) -> Self {
        Self { id: id.into(), mark, label, trail: String::new(), keycap: false, chevron: false, ink }
    }
    pub fn enabled(&self) -> bool { matches!(self.ink, Ink::Plain | Ink::Destructive) }
}

// What the menu knows beyond the listing: its submenu, the anchor it opened on, and the facts the backend sent for it.
#[derive(Default)]
pub struct View {
    pub properties_source: Option<(usize, Json)>,
    pub sub: Sub,
    pub parent: usize,
    pub row: usize,
    pub width: usize,
    pub apps: Option<Vec<(String, String, bool)>>,
    pub formats: Vec<String>,
    pub taildrop: bool,
    pub dropbox: Option<String>,
    pub actions: Vec<(String, String)>,
    pub request: usize,
    pub queued: Option<String>,
    pub hints: bool,
    pub hidden: Vec<String>,
    pub paths: Vec<String>,
    // The folder the menu opened in, so an archive lands beside its items even after the user navigates.
    pub folder: PathBuf,
}
impl View {
    // Settings > Menus hides rows by id; a state with no menu section keeps the shipped hidden set.
    pub fn new(settings: &Json) -> Self {
        let hidden = |state: &Json| state.get("menu").and_then(|menu| menu.get("hidden")).and_then(Json::as_array)
            .map(|ids| ids.iter().filter_map(Json::as_str).map(str::to_owned).collect::<Vec<_>>());
        let hidden = hidden(settings).or_else(|| hidden(&crate::uischema::defaults())).unwrap_or_default();
        Self { hints: flag(settings, "keyHints"), hidden, ..Self::default() }
    }
}

// The frame's top-left cell (0-based), its interior width and rows, and the first entry shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place { pub x: usize, pub y: usize, pub width: usize, pub rows: usize, pub top: usize }

// The rows in the window's file menu groups, None for each rule between groups.
pub fn entries(m: &Model, map: &Map) -> Vec<Option<Entry>> {
    let v = &m.menu_view;
    let tr = |key: &str| m.i18n.tr(key);
    let wait = |ink| if m.menu_ready { ink } else { Ink::Dim };
    let mut groups: Vec<Vec<Entry>> = Vec::new();
    match v.sub {
        Sub::Taildrop => groups.push(if m.taildrop.peers.is_empty() {
            vec![Entry::new("", MACHINE, m.i18n.tr(m.taildrop.state_key()), Ink::Unavailable)]
        } else {
            m.taildrop.peers.iter().enumerate().map(|(i, peer)| Entry::new(&format!("peer:{i}"), MACHINE, peer.label.clone(),
                if m.taildrop.loading() { Ink::Dim } else { wait(Ink::Plain) })).collect()
        }),
        Sub::OpenWith => groups.push(match &v.apps {
            None => vec![Entry::new("", WINDOW, tr("menu.loading"), Ink::Dim)],
            Some(apps) => apps.iter().map(|(id, label, default)| Entry {
                trail: if *default { tr("menu.default") } else { String::new() },
                ..Entry::new(&format!("app:{id}"), WINDOW, label.clone(), Ink::Plain)
            }).collect(),
        }),
        Sub::Compress => groups.push(v.formats.iter().map(|f| Entry::new(&format!("format:{f}"), FILE, format!(".{f}"), Ink::Plain)).collect()),
        Sub::Root => {
            let target = !m.menu_path.as_os_str().is_empty();
            let one = m.menu_count == 1;
            let mut open = Vec::new();
            let mut archive = Vec::new();
            let mut share = Vec::new();
            if target {
                open.push(Entry::new("open", OPEN, tr("menu.open"), wait(Ink::Plain)));
                if v.apps.as_ref().map_or(true, |apps| !apps.is_empty()) {
                    open.push(Entry { chevron: true, ..Entry::new("openwith", WINDOW, tr("menu.openWith"), if one { wait(Ink::Plain) } else { Ink::Dim }) });
                }
                if !m.menu_directory && crate::tui::print::offered(m) {
                    let label = if one { tr("menu.print") } else { m.i18n.trn("menu.row.print_many", m.menu_count as i64) };
                    open.push(Entry::new("print", PRINTER, label, wait(Ink::Plain)));
                }
                if !v.formats.is_empty() { archive.push(Entry { chevron: true, ..Entry::new("compress", ARCHIVE, tr("menu.compress"), wait(Ink::Plain)) }); }
                if !m.menu_directory && crate::tui::convert::offered(m) { archive.push(Entry::new("convert", "\u{eb52}", tr("menu.row.convert"), wait(Ink::Plain))); }
                if !m.menu_directory && crate::tui::combine::offered(m) { archive.push(Entry::new("combine", "\u{eb9f}", tr("menu.row.combine_images"), wait(Ink::Plain))); }
                archive.extend(v.actions.iter().map(|(id, name)| Entry::new(&format!("action:{id}"), HASH, name.clone(), wait(Ink::Plain))));
                if v.taildrop { share.push(taildrop(m, tr("menu.taildrop"))); }
                if let Some(account) = v.dropbox.as_ref().filter(|root| m.menu_view.paths.is_empty() || !m.menu_view.paths.iter().all(|path| within(root, path))) {
                    let mut row = Entry::new("dropbox", DROPBOX, tr("menu.dropbox"), wait(Ink::Plain));
                    // GM 2026-10-29: an unavailable app row is red alone, with no state word, so the menu keeps its size.
                    if !account.starts_with('/') { row.ink = Ink::Unavailable; }
                    share.push(row);
                }
            }
            let delete = Entry::new("delete", TRASH, tr("menu.deletePermanently"), wait(Ink::Destructive));
            let shown = if m.hidden { ("menu.hideHidden", EYE_CLOSED) } else { ("menu.showHidden", EYE) };
            let view = vec![Entry::new("toggleHidden", shown.1, tr(shown.0), Ink::Plain), Entry::new("openTrash", TRASH, tr("menu.openTrash"), Ink::Plain)];
            groups.extend([open, archive, share, if target { vec![delete] } else { Vec::new() }, view]);
            for group in &mut groups { group.retain(|row| !v.hidden.contains(&row.id) || matches!(row.id.as_str(), "open" | "toggleHidden")); }
        }
    }
    let mut out = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !out.is_empty() { out.push(None); }
        for mut row in group {
            if v.hints && row.id != "combine" && row.trail.is_empty() && !row.chevron && row.ink != Ink::Unavailable {
                row.trail = if row.id == "convert" && map.action(&crate::tui::input::Key::character('F', ""), &m.preset).is_empty() { "F".into() } else { super::key_sheet::hint(map, &m.preset, match row.id.as_str() { "delete" => "deletePermanently", id => id }) };
                row.trail = m.keycap_labels.get(&row.trail).cloned().unwrap_or(row.trail);
                row.keycap = !row.trail.is_empty();
            }
            out.push(Some(row));
        }
    }
    out
}
// Send with Taildrop: a folder cannot be sent and an answer with no machine leaves it unavailable, red with no state word (GM 2026-10-29); a check or send in flight waits.
fn taildrop(m: &Model, label: String) -> Entry {
    let mut row = Entry { chevron: true, ..Entry::new("taildrop", TAILSCALE, label, Ink::Plain) };
    let busy = m.taildrop.loading();
    if m.menu_directory || (!busy && m.taildrop.peers.is_empty()) { row.ink = Ink::Unavailable; row.chevron = false; }
    else if busy || !m.menu_ready { row.ink = Ink::Dim; }
    row
}
fn within(root: &str, path: &str) -> bool {
    root.len() > 1 && root.starts_with('/') && (path == root || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/')))
}
fn title(m: &Model) -> Option<String> {
    match m.menu_view.sub {
        Sub::Root => None,
        Sub::OpenWith => Some(m.i18n.tr("menu.title.openWith")),
        Sub::Compress => Some(m.i18n.tr("menu.title.compress")),
        Sub::Taildrop => Some(m.i18n.tr("menu.title.taildrop")),
    }
}
fn needed(rows: &[Option<Entry>], title: Option<&str>) -> usize {
    let longest = rows.iter().flatten().map(|row| 2 + text_width(&row.label)
        + if row.chevron { 3 } else if row.trail.is_empty() { 0 } else { 2 + text_width(&row.trail) }).max().unwrap_or(0);
    2 + longest.max(title.map_or(0, text_width))
}
// Under the target's row at its name column, above it when the body has no room below, clamped to the screen and scrolled when taller than the body.
pub fn place(m: &Model, rows: &[Option<Entry>]) -> Place {
    let title = title(m);
    let (left, middle, _) = crate::tui::layout::panes(m.columns, m.preview_visible);
    let own = needed(rows, title.as_deref());
    let width = if m.menu_view.sub == Sub::Root { own } else { own.max(m.menu_view.width) }.min(m.columns.saturating_sub(2)).max(1);
    let x = (left + 1 + crate::tui::layout::pane_padding(middle)).min(m.columns.saturating_sub(width + 2));
    let body = m.height.max(1);
    let start = crate::tui::layout::BODY_ROW - 1;
    let target = m.visible().iter().position(|index| m.rows.get(index).is_some_and(|row| m.row_path(row) == m.menu_path));
    let row = start + target.unwrap_or(m.menu_view.row).min(body - 1);
    let height = rows.len() + usize::from(title.is_some());
    let shown = height.min(body.saturating_sub(2)).max(1);
    let y = if row + 1 + shown + 2 <= start + body { row + 1 }
        else if row >= start + shown + 2 { row - shown - 2 }
        else { start + body.saturating_sub(shown + 2) };
    let visible = shown.saturating_sub(usize::from(title.is_some())).max(1);
    let top = m.menu_top.min(m.menu_cursor).max((m.menu_cursor + 1).saturating_sub(visible)).min(rows.len().saturating_sub(visible));
    let x = crate::tui::layout::visual_start(m, x, width + 2);
    Place { x, y, width, rows: shown, top }
}
// Some(Some(entry)) on an entry's row, Some(None) elsewhere inside the frame, None outside it; x and y are the terminal's 1-based cells.
pub fn hit(m: &Model, map: &Map, x: usize, y: usize) -> Option<Option<usize>> {
    let rows = entries(m, map);
    let p = place(m, &rows);
    let (x, y) = (x.checked_sub(1)?, y.checked_sub(1)?);
    if x < p.x || x > p.x + p.width + 1 || y < p.y || y > p.y + p.rows + 1 { return None; }
    let first = p.y + 1 + usize::from(m.menu_view.sub != Sub::Root);
    let inside = x > p.x && x <= p.x + p.width && y >= first && y <= p.y + p.rows;
    Some(inside.then(|| p.top + y - first).filter(|index| rows.get(*index).is_some_and(Option::is_some)))
}
pub fn draw(out: &mut String, m: &Model, theme: &Theme, map: &Map) {
    let rows = entries(m, map);
    let p = place(m, &rows);
    let base = theme.base();
    let corners = super::cards::corners(m);
    let at = |out: &mut String, y: usize| out.push_str(&format!("\x1b[{};{}H{}", y + 1, p.x + 1, base));
    at(out, p.y);
    out.push_str(&format!("{}{}{}{}", theme.frame, corners[0], "─".repeat(p.width), corners[1]));
    let mut y = p.y + 1;
    if let Some(title) = title(m) {
        at(out, y);
        out.push_str(&format!("{}│{} {}{} {}│", theme.frame, base, theme.muted, super::cards::field_text(m, &title, p.width.saturating_sub(2)), theme.frame));
        y += 1;
    }
    for (index, row) in rows.iter().enumerate().skip(p.top).take(p.y + p.rows + 1 - y) {
        at(out, y);
        out.push_str(&format!("{}│{}", theme.frame, base));
        match row {
            None => out.push_str(&format!(" {}{} ", theme.rule, "─".repeat(p.width.saturating_sub(2)))),
            Some(row) => {
                if index == m.menu_cursor { out.push_str(&theme.tab); }
                let (mark, ink) = match row.ink {
                    Ink::Plain => (&theme.frame, &theme.foreground), Ink::Dim => (&theme.muted, &theme.muted),
                    Ink::Destructive => (&theme.error, &theme.error), Ink::Unavailable => (&theme.unavailable, &theme.unavailable),
                };
                let trail = if row.chevron {
                    if m.i18n.locale().right_to_left && crate::tui::cells::mirrors("menu_arrows") { "\u{eab5}".into() }
                    else { CHEVRON.to_owned() }
                } else { row.trail.clone() };
                let room = p.width.saturating_sub(4 + if trail.is_empty() { 0 } else { 1 + text_width(&trail) });
                let word = if row.ink == Ink::Unavailable { &theme.unavailable_word } else { &theme.muted };
                let label = super::cards::field_text(m, &row.label, room);
                let label = if row.id.starts_with("peer:") || row.id.starts_with("app:") || row.id.starts_with("action:") {
                    crate::tui::cells::isolate(crate::tui::cells::Run::Name, &label)
                } else { label };
                let shown_trail = fit(&trail, p.width.saturating_sub(5 + room));
                let shown_trail = if row.keycap {
                    crate::tui::cells::isolate(crate::tui::cells::Run::Number, &shown_trail)
                } else { shown_trail };
                if super::cards::rtl(m) {
                    out.push(' ');
                    if !trail.is_empty() { out.push_str(&format!("{word}{shown_trail} ")); }
                    out.push_str(&format!("{ink}{label} {mark}{} ", fit(if m.marks() { row.mark } else { " " }, 1)));
                } else {
                    out.push_str(&format!(" {mark}{} {ink}{label}", fit(if m.marks() { row.mark } else { " " }, 1)));
                    if !trail.is_empty() { out.push_str(&format!(" {word}{shown_trail}")); }
                    out.push(' ');
                }
            }
        }
        out.push_str(&format!("{}{}│", base, theme.frame));
        y += 1;
    }
    at(out, p.y + p.rows + 1);
    out.push_str(&format!("{}{}{}{}{}", theme.frame, corners[2], "─".repeat(p.width), corners[3], base));
}
#[cfg(test)]
#[path = "menu_layout_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "menu_place_tests.rs"]
mod place_tests;
