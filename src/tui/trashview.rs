// Trash as a place: the backend's reviewed Trash list, newest first, never the XDG files folder behind it.
use super::{cells::{ellipsis, fit}, icons, input::Key, keymap::Map, model::{Model, Row}, theme::Theme,
    wire::{count, flag, number, text, word, Wire}};
use crate::jsondoc::Json;
use std::{collections::BTreeMap, io, path::PathBuf};

// The moves that only look: Trash is viewed here, and every action that would change a file is refused with one sentence.
const LOOKS: &[&str] = &["cursorUp", "cursorDown", "cursorFirst", "cursorLast", "cursorFirstArm", "pageUp", "pageDown", "pageForward",
    "halfPageUp", "halfPageDown", "keymapSheet", "togglePreview", "focusPreview", "focusNext", "quit", "quitNoCwd", "pathBar", "goHome", "goConfig", "goDownloads"];

pub struct TrashView {
    id: usize,
    // The folder Trash was opened from, with its cursor, scroll and cursor row, where leaving returns.
    back: PathBuf,
    cursor: usize,
    top: usize,
    current: Option<PathBuf>,
    loaded: bool,
    items: BTreeMap<usize, (String, String)>,
    kinds: crate::backend::kind::Kinds,
}
// Opens Trash from the folder on screen, or lists it again when it is already open.
pub fn open(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    if m.prompts.trash.is_none() {
        if m.searching { w.send(vec![("c", word("searchcancel"))])?; }
        let back = m.search_from.take().unwrap_or_else(|| m.path.clone());
        let view = TrashView { id: 0, back, cursor: m.cursor, top: m.top, current: m.current_path(), loaded: false, items: BTreeMap::new(),
            kinds: crate::backend::kind::Kinds::new() };
        m.invalidate_rows();
        m.filter.clear();
        m.search.clear();
        m.search_query.clear();
        m.searching = false;
        m.parents.clear();
        m.path = view.back.clone();
        (m.total, m.cursor, m.top) = (0, 0, 0);
        m.prompts.trash = Some(view);
    } else {
        m.invalidate_rows();
        if let Some(view) = &mut m.prompts.trash { view.items.clear(); }
    }
    request(m, "list", 0, w)
}
fn request(m: &mut Model, op: &str, start: usize, w: &mut Wire) -> io::Result<()> {
    let Some(view) = &mut m.prompts.trash else { return Ok(()); };
    view.id = view.id.wrapping_add(1).max(1);
    w.send(vec![("c", word("trashbrowse")), ("op", word(op)), ("id", number(view.id)), ("start", number(start)), ("count", number(m.height))])
}
// The same scroll rule as a folder's window, fetching only rows Trash has not sent yet.
pub fn window(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    let old_top = m.top;
    if m.cursor < m.top { m.top = m.cursor; }
    if m.cursor >= m.top + m.height { m.top = m.cursor + 1 - m.height; }
    if m.top != old_top { m.scrolled_at = Some(std::time::Instant::now()); }
    if (m.top..(m.top + m.height).min(m.total)).all(|index| m.rows.contains_key(&index)) { return Ok(()); }
    let top = m.top;
    request(m, "window", top, w)
}
// Sample input: {"t":"trashbrowse","id":1,"op":"list","ok":true,"total":1,"start":0,"rows":[{"uri":"trash:///a.txt","original":"/w/a.txt","size":13,"deleted":"2025-06-14T09:30:00","directory":false,"icon":"text-plain"}]}.
pub fn receive(m: &mut Model, value: &Json) {
    let Some(view) = m.prompts.trash.as_mut().filter(|view| view.id == count(value, "id")) else { return; };
    view.loaded = true;
    if !flag(value, "ok") {
        let error = text(value, "error").to_owned();
        m.fail(error);
        return;
    }
    m.total = count(value, "total");
    let start = count(value, "start");
    for (offset, row) in value.get("rows").and_then(Json::as_array).unwrap_or(&[]).iter().enumerate() {
        let original = text(row, "original");
        let directory = flag(row, "directory");
        let icon = text(row, "icon");
        let kind = if directory { m.prompts.i18n.tr("trash.kindFolder") } else {
            icon.split_once('-').and_then(|(major, minor)| view.kinds.comment(&format!("{major}/{minor}"))).unwrap_or_else(|| m.prompts.i18n.tr("trash.kindFile"))
        };
        let name = original.rsplit('/').next().unwrap_or(original).to_owned();
        m.rows.insert(start + offset, Row { name, directory, size: count(row, "size"), mode: if directory { 0o40755 } else { 0o100644 },
            link: String::new(), kind, thumbnail: false, modified: 0, icon: icon.into() });
        view.items.insert(start + offset, (original.into(), text(row, "deleted").into()));
    }
    m.cursor = m.cursor.min(m.total.saturating_sub(1));
}
// Leaves Trash for the folder it was opened from, with that folder's cursor, scroll and cursor row.
fn leave(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    let view = m.prompts.trash.take().unwrap();
    m.invalidate_rows();
    m.total = 0;
    m.restore_cursor = Some(view.cursor);
    m.restore_path = view.current;
    m.top = view.top;
    m.path = view.back.clone();
    m.open(view.back, w)
}
// In Trash a key either looks, leaves, lists again, or is refused; true when this took the key. Armed is the prefix the key before it left.
pub fn refuses(m: &mut Model, key: &Key, map: &Map, armed: &str, w: &mut Wire) -> io::Result<bool> {
    if m.prompts.trash.is_none() || m.editor.is_some() || m.sheet || m.menu || m.deletion.is_some() || m.quicklook { return Ok(false); }
    // Only a press on the preview pane reaches the focused preview; every other press would land on the listing.
    if let Some(pointer) = &key.pointer {
        let (left, middle, _) = super::render::panes(m.columns, m.preview_visible);
        let body = pointer.y >= super::render::BODY_ROW && pointer.y < super::render::BODY_ROW + m.height;
        return Ok(!(m.preview_focus && m.preview_visible && body && pointer.x > left + middle + 2));
    }
    if m.preview_focus { return Ok(false); }
    let action = match armed.strip_prefix("prefix:") {
        Some(prefix) => map.sequence(prefix, key, &m.preset, "listing"),
        None => map.action(key, &m.preset),
    };
    match action.as_str() {
        "escape" if !m.error.is_empty() => Ok(false),
        "escape" | "parent" | "historyBack" => leave(m, w).map(|_| true),
        "reload" => open(m, w).map(|_| true),
        "" if key.mods.is_empty() && key.text == "q" => Ok(false),
        "" => Ok(true),
        action if LOOKS.contains(&action) || action.starts_with("prefix:") => Ok(false),
        _ => {
            m.key_arm.clear();
            m.fail(m.prompts.i18n.tr("trash.viewOnly"));
            Ok(true)
        }
    }
}
// No path stands for a Trash row, so nothing previews, opens or acts on one through the folder it was opened from.
pub fn hides(m: &Model) -> bool { m.prompts.trash.is_some() }
// Trash's disk facts are the folder's it was opened from.
pub fn opened_from(m: &Model) -> Option<&PathBuf> { m.prompts.trash.as_ref().map(|view| &view.back) }
// Until Trash answers, the pane stays empty instead of showing an empty folder's notice.
pub fn settled(m: &Model) -> bool { m.prompts.trash.as_ref().map_or(true, |view| view.loaded) }
pub fn place(m: &Model) -> Option<String> { m.prompts.trash.as_ref().map(|_| m.prompts.i18n.tr("trash.place")) }
pub fn sort(m: &Model) -> Option<String> { m.prompts.trash.as_ref().map(|_| m.prompts.i18n.tr("trash.sort")) }
// A Trash row's detail column is the date it went in.
pub fn detail(m: &Model, index: usize, entry: &Row) -> String {
    match m.prompts.trash.as_ref() {
        Some(view) => view.items.get(&index).map(|(_, deleted)| super::calendar::value(&Json::Str(deleted.clone()))).unwrap_or_default(),
        None => super::detail::value(m, index, entry),
    }
}
// The preview's head: the name, kind and size, then the window's two Trash facts and a blank row.
pub fn preview(m: &Model, y: usize, width: usize, theme: &Theme) -> Option<String> {
    let view = m.prompts.trash.as_ref()?;
    let (Some(row), Some((original, deleted))) = (m.rows.get(&m.cursor), view.items.get(&m.cursor)) else { return Some(fit("", width)); };
    let i18n = &m.prompts.i18n;
    let location = std::path::Path::new(original).parent().map(|parent| {
        let home = std::env::var_os("HOME").filter(|home| !home.is_empty()).map(PathBuf::from);
        match home.as_ref().and_then(|home| parent.strip_prefix(home).ok()) {
            Some(tail) if tail.as_os_str().is_empty() => "~".into(),
            Some(tail) => format!("~/{}", tail.to_string_lossy()),
            None => parent.to_string_lossy().into_owned(),
        }
    }).unwrap_or_default();
    Some(match y {
        0 => {
            let mark = if m.marks() { format!("{} ", icons::row(row, true)) } else { String::new() };
            format!("\x1b[1m{}\x1b[22m", ellipsis(&format!("{mark}{}", row.name), width))
        }
        1 => format!("{}{}", theme.meta, ellipsis(&format!("{} · {}", row.kind, super::render::bytes(row.size)), width)),
        2 => format!("{}{}", theme.meta, ellipsis(&i18n.tr_args("trash.deleted", &[&super::calendar::value(&Json::Str(deleted.clone()))]), width)),
        3 => format!("{}{}", theme.meta, ellipsis(&i18n.tr_args("trash.original", &[&location]), width)),
        _ => fit("", width),
    })
}

#[cfg(test)]
#[path = "trashview_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "trashview_keys_tests.rs"]
mod keys_tests;
