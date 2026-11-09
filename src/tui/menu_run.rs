//! The file menu's keys, pointer and rows at work: opening it, stepping it, and each row's request and answer.
use super::menu_layout::{entries, hit, place, Entry, Sub};
use crate::tui::{input::{Key, Pointer, WHEEL_DOWN, WHEEL_UP}, keymap::Map, model::Model, wire::{count, flag, number, text, word, Wire}};
use crate::jsondoc::Json;
use std::io;



// Opens the menu on the cursor row, asking the backend for its identity snapshot and for the facts its rows read.
pub fn open(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    m.menu = true;
    m.menu_cursor = 0;
    m.menu_top = 0;
    m.taildrop.refresh_locale(&m.i18n);
    m.taildrop.refresh();
    m.menu_ready = false;
    m.menu_action.clear();
    m.menu_path = m.current_path().unwrap_or_default();
    m.menu_directory = m.rows.get(&m.cursor).is_some_and(|row| row.directory);
    let folder = m.path.clone();
    let row = m.visible().iter().position(|index| *index == m.cursor).unwrap_or(0);
    let view = &mut m.menu_view;
    view.folder = folder;
    view.sub = Sub::Root;
    view.apps = None;
    view.queued = None;
    view.actions.clear();
    view.row = row;
    super::cards::probe(&m.rounded);
    let rows: Vec<usize> = if m.selected.is_empty() { m.rows.contains_key(&m.cursor).then_some(m.cursor).into_iter().collect() } else { m.selected.iter().copied().collect() };
    m.menu_count = rows.len();
    m.action_id = m.action_id.wrapping_add(1).max(1);
    m.menu_view.request = m.action_id;
    w.send(vec![("c", word("formats")), ("id", number(m.action_id))])?;
    if rows.is_empty() { return Ok(()); }
    let targets = &rows[..m.menu_count];
    m.menu_directory = targets.iter().filter_map(|index| m.selected_rows.get(index).or_else(|| m.rows.get(index))).any(|row| row.directory);
    m.menu_view.paths = targets.iter().filter_map(|index| m.selected_rows.get(index).or_else(|| m.rows.get(index))).map(|row| m.row_path(row).to_string_lossy().into_owned()).collect();
    // A single target is what the menu acts on, so a selected folder under a file cursor is the target.
    if let [only] = m.menu_view.paths.as_slice() { m.menu_path = only.into(); }
    let paths = Json::Arr(m.menu_view.paths.iter().map(|path| word(path)).collect());
    w.send(vec![("c", word("actions")), ("op", word("list")), ("id", number(m.action_id)), ("paths", paths)])?;
    m.menu_action = "menu".into();
    w.send(vec![("c", word("menuaction")), ("op", word("snapshot")), ("id", number(m.action_id)), ("rows", Json::Arr(rows.into_iter().map(number).collect())), ("cursor", number(m.cursor))])
}
// The snapshot answered: one item also asks for its Open With registry, as the window does.
pub fn ready(m: &mut Model, w: &mut Wire) -> io::Result<()> {
    m.menu_ready = true;
    if m.menu && m.menu_count == 1 && !m.menu_path.as_os_str().is_empty() && !m.menu_view.hidden.iter().any(|id| id == "openwith") {
        m.menu_action = "menuApps".into();
        w.send(vec![("c", word("menuaction")), ("op", word("applications")), ("id", number(m.action_id))])?;
    }
    Ok(())
}
// The backend's formats and Actions answers for the open menu.
// Sample input: {"t":"formats","id":7,"archive":["zip"],"providers":{"taildrop":{"installed":false},"dropbox":{"installed":true},"dropboxInfo":"{\"personal\":{\"path\":\"/home/gm/Dropbox\"}}","dropboxError":""}}; the facts answer the rows read.
pub fn receive(m: &mut Model, value: &Json) {
    if count(value, "id") != m.menu_view.request { return; }
    let list = |key: &str| value.get(key).and_then(Json::as_array).unwrap_or(&[]);
    if text(value, "t") == "formats" {
        m.menu_view.formats = list("archive").iter().filter_map(Json::as_str).map(str::to_owned).collect();
        let providers = value.get("providers").unwrap_or(&Json::Null);
        m.menu_view.taildrop = providers.get("taildrop").is_some_and(|p| flag(p, "installed"));
        m.menu_view.dropbox = providers.get("dropbox").filter(|p| flag(p, "installed"))
            .map(|_| dropbox(text(providers, "dropboxInfo"), text(providers, "dropboxError")));
    } else if !text(value, "error").is_empty() {
        m.fail(text(value, "error").into());
    } else {
        m.menu_view.actions = list("actions").iter().map(|a| (text(a, "id").to_owned(), text(a, "name").to_owned())).collect();
    }
}
// Sample input: {"personal":{"path":"/home/gm/Dropbox"}}; the account folder, or the state word's key when there is none.
fn dropbox(info: &str, error: &str) -> String {
    if !error.is_empty() { return "menu.state.notRunning".into(); }
    let Ok(data) = crate::jsondoc::parse(info) else { return if info.is_empty() { "menu.state.signedOut" } else { "menu.state.notRunning" }.into() };
    ["personal", "business"].iter().find_map(|kind| data.get(kind).map(|account| text(account, "path").trim_end_matches('/').to_owned()))
        .map(|path| if path.starts_with('/') { path } else { "menu.state.notRunning".into() })
        .unwrap_or_else(|| "menu.state.signedOut".into())
}
pub fn key(m: &mut Model, key: &Key, map: &Map, w: &mut Wire) -> io::Result<()> {
    if let Some(pointer) = &key.pointer { return pointer_key(m, pointer, map, w); }
    let action = map.in_context(key, &m.preset, "menu");
    let action = if super::cards::rtl(m) && crate::tui::cells::mirrors("navigation") && key.name != "Backspace" {
        match action.as_str() { "menuRight" => "parent".into(), "parent" => "menuRight".into(), _ => action }
    } else { action };
    let rows = entries(m, map);
    match action.as_str() {
        "escape" | "parent" => back(m),
        "cursorDown" | "cursorUp" | "focusNext" | "focusPrevious" => {
            let forward = matches!(action.as_str(), "cursorDown" | "focusNext");
            for _ in 0..rows.len() {
                m.menu_cursor = (m.menu_cursor + if forward { 1 } else { rows.len() - 1 }) % rows.len().max(1);
                if rows.get(m.menu_cursor).is_some_and(|row| row.as_ref().is_some_and(Entry::enabled)) { break; }
            }
            m.menu_top = place(m, &rows).top;
        }
        "open" | "preview" | "menuRight" => return activate(m, map, w, action == "menuRight"),
        // The window's Copy As letters copy the menu's targets here too, after a registry request still answering.
        "copyPath" | "copyRelative" | "copyName" | "copyStem" | "copydirpath" | "copyUri" | "copyQuoted" => {
            if m.menu_action == "menuApps" { m.menu_view.queued = Some(action); return Ok(()); }
            return run(m, &action, w);
        }
        _ if !action.is_empty() => m.fail_text(crate::tui::parity::refusal_text(&action)),
        _ => {}
    }
    Ok(())
}
fn back(m: &mut Model) {
    if m.menu_view.sub == Sub::Root {
        m.menu = false;
        m.menu_action.clear();
    } else {
        m.menu_view.sub = Sub::Root;
        m.menu_cursor = m.menu_view.parent;
        m.menu_top = 0;
    }
}
fn pointer_key(m: &mut Model, pointer: &Pointer, map: &Map, w: &mut Wire) -> io::Result<()> {
    if pointer.released { return Ok(()); }
    if matches!(pointer.button, WHEEL_UP | WHEEL_DOWN) { return key(m, &Key::named(if pointer.button == WHEEL_UP { "Up" } else { "Down" }, ""), map, w); }
    match hit(m, map, pointer.x, pointer.y) {
        Some(Some(index)) if (pointer.motion || pointer.button == 0) && entries(m, map)[index].as_ref().is_some_and(Entry::enabled) => {
            m.menu_cursor = index;
            if !pointer.motion { return activate(m, map, w, false); }
        }
        None if !pointer.motion && pointer.button == 0 => {
            m.menu = false;
            m.menu_action.clear();
        }
        _ => {}
    }
    Ok(())
}
fn activate(m: &mut Model, map: &Map, w: &mut Wire, enter_only: bool) -> io::Result<()> {
    let rows = entries(m, map);
    let Some(row) = rows.get(m.menu_cursor).cloned().flatten().filter(Entry::enabled) else { return Ok(()); };
    if row.chevron {
        let sub = match row.id.as_str() { "openwith" => Sub::OpenWith, "compress" => Sub::Compress, _ => Sub::Taildrop };
        m.menu_view.parent = m.menu_cursor;
        m.menu_view.width = place(m, &rows).width;
        m.menu_view.sub = sub;
        m.menu_cursor = 0;
        m.menu_top = 0;
        return Ok(());
    }
    if enter_only && m.menu_view.sub == Sub::Root { return Ok(()); }
    // One menu request runs at a time, so a row chosen while the registry is still answering goes out after it.
    if m.menu_action == "menuApps" {
        m.menu_view.queued = Some(row.id);
        return Ok(());
    }
    run(m, &row.id, w)
}
fn run(m: &mut Model, id: &str, w: &mut Wire) -> io::Result<()> {
    let request = m.action_id;
    let menu = |op: &str, extra: Vec<(&'static str, Json)>| {
        let mut fields = vec![("c", word("menuaction")), ("op", word(op)), ("id", number(request))];
        fields.extend(extra);
        fields
    };
    if let Some(index) = id.strip_prefix("peer:").and_then(|i| i.parse::<usize>().ok()) {
        if m.pending_clipboard || m.taildrop_target.is_some() { return Ok(()); }
        if let Some(peer) = m.taildrop.peers.get(index) {
            m.taildrop_target = Some(peer.clone());
            m.menu_action = "taildropRefresh".into();
            m.taildrop.refresh();
            m.say_key("menu.taildropChecking");
            m.menu = false;
        }
        return Ok(());
    }
    m.menu = false;
    let fields = if let Some(app) = id.strip_prefix("app:") {
        m.menu_action = "openWith".into();
        menu("openWith", vec![("application", word(app))])
    } else if let Some(format) = id.strip_prefix("format:") {
        m.menu_action = "compress".into();
        menu("activate", vec![("action", word(&format!("compress:{format}")))])
    } else if let Some(action) = id.strip_prefix("action:") {
        m.menu_action.clear();
        let paths = Json::Arr(m.menu_view.paths.iter().map(|path| word(path)).collect());
        vec![("c", word("actions")), ("op", word("run")), ("id", number(request)), ("action", word(action)), ("paths", paths)]
    } else {
        match id {
            "combine" => return crate::tui::combine::open(m, w, true),
            "convert" => return crate::tui::convert::open(m, w, true),
            "print" => return crate::tui::print::open(m, w, true),
            "open" => { m.menu_action = "open".into(); menu("validate", vec![("action", word("open"))]) }
            "dropbox" => {
                m.menu_action = "dropbox".into();
                menu("providerDestination", vec![("dest", word(m.menu_view.dropbox.as_deref().unwrap_or_default()))])
            }
            _ => {
                m.menu_action.clear();
                return crate::tui::actions::act(m, match id { "delete" => "deletePermanently", other => other }, w);
            }
        }
    };
    w.send(fields)
}
// The menu slot's answers this module asked for; false hands the reply back to the model's own branches.
// Sample input: {"t":"menuaction","op":"activate","id":7,"ok":true,"action":"compress:zip","paths":["/fx/v/work/notes.txt"],"dest":""}; the answer to an activate.
pub fn reply(m: &mut Model, value: &Json, w: &mut Wire) -> io::Result<bool> {
    let close = |m: &mut Model, w: &mut Wire| { m.menu_action.clear(); w.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", number(m.action_id))]) };
    match (text(value, "op"), m.menu_action.as_str()) {
        ("applications", "menuApps") => {
            m.menu_action = "menu".into();
            let apps = value.get("applications").and_then(Json::as_array).unwrap_or(&[]);
            m.menu_view.apps = Some(if flag(value, "ok") { apps.iter().map(|a| (text(a, "id").into(), text(a, "label").into(), flag(a, "default"))).collect() } else { Vec::new() });
            if let Some(id) = m.menu_view.queued.take() { run(m, &id, w)?; }
        }
        (_, "openWith" | "compress" | "dropbox" | "dropboxMove") if !flag(value, "ok") => {
            m.fail(text(value, "error").into());
            close(m, w)?;
        }
        ("openWith", "openWith") => close(m, w)?,
        ("providerDestination", "dropbox") => {
            m.menu_action = "dropboxMove".into();
            let dest = word(m.menu_view.dropbox.as_deref().unwrap_or_default());
            w.send(vec![("c", word("menuaction")), ("op", word("activate")), ("id", number(m.action_id)), ("action", word("dropbox")), ("dest", dest)])?;
        }
        ("activate", "dropboxMove") => {
            m.menu_action.clear();
            let dest = word(m.menu_view.dropbox.as_deref().unwrap_or_default());
            w.send(vec![("c", word("transfer")), ("op", word("move")), ("rows", m.indices()), ("dest", dest), ("menuId", number(m.action_id))])?;
        }
        ("activate", "compress") => {
            m.menu_action.clear();
            let paths: Vec<String> = value.get("paths").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(str::to_owned).collect();
            let format = text(value, "action").strip_prefix("compress:").unwrap_or_default().to_owned();
            let name = |path: &str| path.rsplit('/').next().unwrap_or_default().to_owned();
            let stem = match paths.as_slice() {
                [one] => { let leaf = name(one); leaf.rfind('.').filter(|cut| *cut > 0).map_or(leaf.clone(), |cut| leaf[..cut].to_owned()) }
                _ => m.menu_view.folder.file_name().map_or("archive".into(), |leaf| leaf.to_string_lossy().into_owned()),
            };
            let dest = m.menu_view.folder.join(format!("{stem}.{format}"));
            m.say_text(crate::tui::words::Text::Plural("menu.compressing", paths.len(), vec![format.clone().into()]));
            w.send(vec![("c", word("archive")), ("op", word("compress")), ("paths", Json::Arr(paths.iter().map(|p| word(p)).collect())),
                ("dest", word(&dest.to_string_lossy())), ("format", word(&format)), ("menuId", number(m.action_id))])?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

#[cfg(test)]
#[path = "menu_run_tests.rs"]
mod tests;
