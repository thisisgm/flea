use super::{
    editor::Editor,
    input::Key,
    keymap::Map,
    model::{Model, Tab},
    wire::{word, Wire},
};
use crate::jsondoc::Json;
use std::{io, path::PathBuf};

pub fn key(model: &mut Model, key: &Key, map: &Map, wire: &mut Wire) -> io::Result<()> {
    let armed = std::mem::take(&mut model.key_arm);
    if model.deletion.is_some() { return deletion_key(model, key, wire); }
    if !model.menu && !model.menu_action.is_empty() && model.menu_action != "menu" {
        let action = map.action(key, &model.preset);
        return if matches!(action.as_str(), "escape" | "quit") { act(model, &action, wire) } else { Ok(()) };
    }
    if let Some(pointer) = &key.pointer {
        return pointer_key(model, key, pointer, map, wire);
    }
    if model.editor.is_some() {
        return edit(model, key, wire);
    }
    if model.sheet {
        let action = map.in_context(key, &model.preset, "panel");
        if action == "escape" || (key.mods.is_empty() && key.text == "?") {
            model.sheet = false;
            model.properties = None;
        } else if action == "cursorDown" {
            model.sheet_top =
                (model.sheet_top + 1).min(super::render::panel_rows(model, map).len().saturating_sub(1));
        } else if action == "cursorUp" {
            model.sheet_top = model.sheet_top.saturating_sub(1);
        }
        return Ok(());
    }
    if model.menu {
        let count = if model.taildrop.submenu {
            model.taildrop.peers.len().max(1)
        } else {
            3
        };
        let action = map.in_context(key, &model.preset, "menu");
        match action.as_str() {
            "escape" | "parent" => {
                if model.taildrop.submenu {
                    model.taildrop.submenu = false;
                    model.menu_cursor = 2;
                    model.menu_top = 0;
                } else {
                    model.menu = false;
                    model.menu_action.clear();
                }
            }
            "cursorDown" | "cursorUp" | "focusNext" | "focusPrevious" => {
                for _ in 0..count {
                    model.menu_cursor = (model.menu_cursor + if matches!(action.as_str(), "cursorDown" | "focusNext") { 1 } else { count - 1 }) % count;
                    if model.menu_enabled(model.menu_cursor) { break; }
                }
                model.menu_top = model.menu_top.min(model.menu_cursor);
                let visible = model.height.max(1);
                if model.menu_cursor >= model.menu_top + visible { model.menu_top = model.menu_cursor + 1 - visible; }
            }
            "open" | "preview" | "menuRight" => {
                if !model.menu_enabled(model.menu_cursor) { return Ok(()); }
                if model.taildrop.submenu {
                    if model.pending_clipboard || model.taildrop_target.is_some() {
                        return Ok(());
                    }
                    if let Some(peer) = model.taildrop.peers.get(model.menu_cursor) {
                        model.taildrop_target = Some(peer.clone());
                        model.menu_action = "taildropRefresh".into();
                        model.taildrop.refresh();
                        model.say("Checking Taildrop".into());
                        model.menu = false;
                    }
                    return Ok(());
                }
                if model.menu_cursor == 2 {
                    if model.taildrop.peers.is_empty() {
                        model.fail(if model.taildrop.loading() {
                            "Taildrop is still loading".into()
                        } else if !model.taildrop.error.is_empty() {
                            model.taildrop.error.clone()
                        } else {
                            "No reachable Taildrop devices".into()
                        });
                    } else if model.rows.contains_key(&model.cursor) {
                        model.taildrop.submenu = true;
                        model.menu_cursor = 0;
                        model.menu_top = 0;
                    }
                    return Ok(());
                }
                if action == "menuRight" {
                    return Ok(());
                }
                model.menu = false;
                if model.menu_cursor == 0 {
                    model.menu_action = "open".into();
                    return wire.send(vec![("c", word("menuaction")), ("op", word("validate")), ("id", super::wire::number(model.action_id)), ("action", word("open"))]);
                }
                model.menu_action.clear();
                return act(model, "toggleHidden", wire);
            }
            _ => {}
        }
        return Ok(());
    }
    if key.name == "P" && key.mods == "alt" {
        model.preview_visible = !model.preview_visible;
        model.preview_focus = false;
        save_preview_column(model);
        return Ok(());
    }
    if key.name == "Space" && key.mods == "ctrl" {
        super::preview::load(model, true);
        return Ok(());
    }
    if model.preview_focus || model.quicklook {
        let context = if model.pdf.is_some() {
            "pdf"
        } else if model.player.is_some() {
            "media"
        } else {
            "preview"
        };
        let action = map.in_context(key, &model.preset, context);
        if action == "escape" || action == "focusPreview" {
            model.preview_focus = false;
            model.quicklook = false;
            if let Some(pdf) = &mut model.pdf {
                if pdf.control >= 5 {
                    pdf.control = 0;
                }
            }
            return Ok(());
        }
        if let Some(pdf) = &mut model.pdf {
            let controls = if model.quicklook { 6 } else { 5 };
            match action.as_str() {
                "focusNext" | "focusPrevious" => {
                    pdf.control = if action == "focusPrevious" {
                        (pdf.control + controls - 1) % controls
                    } else {
                        (pdf.control + 1) % controls
                    }
                }
                "seekBack" => pdf.turn(-1),
                "seekForward" | "pageForward" => pdf.turn(1),
                "cursorUp" => pdf.scroll(-1),
                "cursorDown" => pdf.scroll(1),
                "zoomOut" => pdf.zoom(-1),
                "zoomIn" => pdf.zoom(1),
                "expand" => model.quicklook = true,
                "open" | "preview" => match pdf.control {
                    0 => pdf.turn(-1),
                    1 => pdf.turn(1),
                    2 => pdf.zoom(-1),
                    3 => pdf.zoom(1),
                    4 => model.quicklook = true,
                    5 => {
                        model.quicklook = false;
                        model.preview_focus = false;
                        pdf.control = 0;
                    }
                    _ => {}
                },
                _ => {}
            }
        } else if let Some(player) = &mut model.player {
            match action.as_str() {
                "focusNext" | "focusPrevious" => player.control = 1 - player.control,
                "preview" => player.toggle()?,
                "open" => {
                    if player.control == 0 {
                        player.toggle()?;
                    }
                }
                "seekBack" => {
                    if player.control == 1 {
                        player.seek(-5)?;
                    }
                }
                "seekForward" | "pageForward" => {
                    if player.control == 1 {
                        player.seek(5)?;
                    }
                }
                _ => {}
            }
        } else {
            match action.as_str() {
                "cursorDown" => {
                    model.preview_scroll =
                        (model.preview_scroll + 1).min(model.preview_line_count.saturating_sub(1))
                }
                "cursorUp" => model.preview_scroll = model.preview_scroll.saturating_sub(1),
                _ => {}
            }
        }
        return Ok(());
    }
    if key.name == "Paste" {
        return Ok(());
    }
    if key.mods.is_empty() && key.text.len() == 1 {
        if let Ok(n) = key.text.parse::<usize>() {
            if (1..=9).contains(&n) {
                return tab(model, n - 1, wire);
            }
        }
    }
    if key.name == "PageDown" && key.mods == "ctrl" {
        return tab(model, (model.tab + 1) % model.tabs.len(), wire);
    }
    if key.name == "PageUp" && key.mods == "ctrl" {
        return tab(
            model,
            (model.tab + model.tabs.len() - 1) % model.tabs.len(),
            wire,
        );
    }
    if key.name == "Tab" && key.mods == "ctrl" {
        model.preview_focus = model.preview_visible;
        return Ok(());
    }
    let mut action = map.action(key, &model.preset);
    if matches!(
        action.as_str(),
        "copyArm" | "cutArm" | "pasteArm" | "cursorFirstArm" | "trashArm"
    ) {
        const TRASH_ARM: std::time::Duration = std::time::Duration::from_millis(1500);
        let expired = action == "trashArm"
            && model
                .trash_armed_at
                .map_or(true, |at| at.elapsed() >= TRASH_ARM);
        if armed != action || expired {
            if action == "trashArm" {
                model.trash_armed_at = Some(std::time::Instant::now());
                model.say("Press d again to trash, or Delete on its own.".into());
            }
            model.key_arm = action;
            return Ok(());
        }
        action = match action.as_str() {
            "copyArm" => "copy",
            "cutArm" => "cut",
            "pasteArm" => "paste",
            "cursorFirstArm" => "cursorFirst",
            _ => "trash",
        }
        .into();
    }
    if action.is_empty() {
        if !key.mods.is_empty() {
            return Ok(());
        }
        match key.text.as_str() {
            "q" => model.quit = true,
            "H" if matches!(model.preset.as_str(), "default" | "vim") => {
                history(model, false, wire)?
            }
            "L" if matches!(model.preset.as_str(), "default" | "vim") => {
                history(model, true, wire)?
            }
            _ => {}
        }
        return Ok(());
    }
    act(model, &action, wire)
}
fn act(m: &mut Model, action: &str, w: &mut Wire) -> io::Result<()> {
    if !m.menu_action.is_empty() && m.menu_action != "menu" && !matches!(action, "quit" | "escape") { return Ok(()); }
    if m.pending.is_some() && !matches!(action, "quit" | "escape" | "keymapSheet") {
        return Ok(());
    }
    if m.searching
        && matches!(
            action,
            "paste" | "movePaste" | "duplicate" | "trash" | "trashArm" | "deletePermanently" | "rename" | "copy" | "cut"
        )
    {
        m.fail("Wait for search to settle before a file operation".into());
        return Ok(());
    }
    if !m.rows.contains_key(&m.cursor)
        && matches!(
            action,
            "open"
                | "duplicate"
                | "copy"
                | "cut"
                | "trash"
                | "trashArm"
                | "deletePermanently"
                | "rename"
                | "preview"
                | "reveal"
                | "toggleSelect"
        )
    {
        return Ok(());
    }
    match action {
        "cursorDown" => m.move_by(1, false, w)?,
        "cursorUp" => m.move_by(-1, false, w)?,
        "extendDown" => m.move_by(1, true, w)?,
        "extendUp" => m.move_by(-1, true, w)?,
        "pageDown" => m.move_by(m.height as isize, false, w)?,
        "pageUp" => m.move_by(-(m.height as isize), false, w)?,
        "first" | "cursorFirst" => {
            m.cursor = 0;
            m.window(w)?;
        }
        "last" | "cursorLast" => {
            m.cursor = m.total.saturating_sub(1);
            m.window(w)?;
        }
        "parent" => {
            if let Some(p) = m.path.parent().map(|p| p.to_path_buf()) {
                navigate(m, p, w)?;
            }
        }
        "historyBack" => history(m, false, w)?,
        "historyForward" => history(m, true, w)?,
        "open" => {
            if let Some(path) = m.current_path() {
                if m.rows.get(&m.cursor).is_some_and(|r| r.directory) {
                    navigate(m, path, w)?;
                } else if crate::open::open(&path.to_string_lossy()) != 0 {
                    m.fail("Could not open selected file".into());
                }
            }
        }
        "toggleSelect" => {
            if m.rows.contains_key(&m.cursor) && !m.selected.remove(&m.cursor) {
                m.selected.insert(m.cursor);
            }
        }
        "selectAll" => {
            m.selected = if m.filter.is_empty() {
                (0..m.total).collect()
            } else {
                m.shown().into_iter().collect()
            };
        }
        "toggleHidden" => {
            m.hidden = !m.hidden;
            save("hidden", Json::Bool(m.hidden), m);
            m.open(m.path.clone(), w)?;
        }
        "pathBar" => {
            m.editor = Some(Editor::new("path", String::new(), m.path.clone()));
            m.completion = m.completer.request("", &m.path);
        }
        "filter" => m.editor = Some(Editor::new("filter", m.filter.clone(), m.path.clone())),
        "search" => m.editor = Some(Editor::new("search", String::new(), m.path.clone())),
        "rename" => {
            if m.selected.len() > 1 {
                m.action_id = m.action_id.wrapping_add(1).max(1);
                m.menu_action = "bulkSnapshot".into();
                m.menu_count = m.selected.len();
                w.send(vec![("c", word("menuaction")), ("op", word("snapshot")), ("id", super::wire::number(m.action_id)), ("rows", m.indices())])?;
            } else if let Some((index, row)) = m.single_row() {
                m.cursor = index;
                m.window(w)?;
                m.editor = Some(Editor::rename(
                    row.name.clone(),
                    m.row_path(&row),
                    row.directory,
                )?);
            } else {
                m.fail("Selected item is unavailable; select it again to rename".into());
            }
        }
        "newFolder" => m.editor = Some(Editor::new("mkdir", "New Folder".into(), m.path.clone())),
        "copy" | "cut" => {
            if m.pending_clipboard || m.taildrop_target.is_some() {
                return Ok(());
            }
            m.cut = action == "cut";
            m.pending_clipboard = true;
            w.send(vec![("c", word("paths")), ("rows", m.indices())])?;
        }
        "paste" | "movePaste" => {
            if m.clipboard.is_empty() {
                m.say("Nothing to paste".into());
            } else {
                w.send(vec![
                    ("c", word("transfer")),
                    (
                        "op",
                        word(if m.cut || action == "movePaste" {
                            "move"
                        } else {
                            "copy"
                        }),
                    ),
                    (
                        "paths",
                        Json::Arr(m.clipboard.iter().map(|p| word(p)).collect()),
                    ),
                    ("dest", word(&m.path.to_string_lossy())),
                ])?;
            }
        }
        "trash" | "trashArm" => {
            if m.total > 0 {
                w.send(vec![("c", word("trash")), ("rows", m.indices())])?;
            }
        }
        "undo" | "redo" => w.send(vec![("c", word(action))])?,
        "deletePermanently" => {
            m.action_id = m.action_id.wrapping_add(1).max(1);
            m.menu_action = "deleteSnapshot".into();
            m.deletion = Some(super::model::Deletion::default());
            m.delete_marks.clear();
            let indices: Vec<usize> = if m.selected.is_empty() { vec![m.cursor] } else { m.selected.iter().copied().collect() };
            for index in indices {
                if let Some(row) = m.selected_rows.get(&index).or_else(|| m.rows.get(&index)) {
                    m.delete_marks.insert(m.row_path(row), row.clone());
                }
            }
            w.send(vec![("c", word("menuaction")), ("op", word("snapshot")), ("id", super::wire::number(m.action_id)), ("rows", m.indices())])?;
        }
        "duplicate" => {
            if let Some((_, row)) = m.single_row() {
                let path = m.row_path(&row);
                w.send(vec![
                    ("c", word("duplicate")),
                    ("path", word(&path.to_string_lossy())),
                ])?;
            } else {
                m.fail("Select one available item to duplicate".into());
            }
        }
        "newFile" => {
            m.action_id = m.action_id.wrapping_add(1).max(1);
            m.editor = Some(Editor::new("newfile", "New File".into(), m.path.clone()));
        }
        "sortNext" | "sortReverse" => {
            m.restore_path = m.current_path();
            if action == "sortReverse" {
                m.reverse = !m.reverse;
            } else {
                m.sort = match m.sort.as_str() {
                    "name" => "size",
                    "size" => "date",
                    "date" => "kind",
                    _ => "name",
                }
                .into();
                m.reverse = false;
            }
            w.send(vec![
                ("c", word("sort")),
                ("by", word(&m.sort)),
                ("desc", Json::Bool(m.reverse)),
                ("foldersFirst", Json::Bool(m.folders_first)),
                ("groupByKind", Json::Bool(m.group_by_kind)),
            ])?;
            m.invalidate_rows();
            save(
                "sort",
                Json::Obj(vec![
                    ("key".into(), word(&m.sort)),
                    ("reverse".into(), Json::Bool(m.reverse)),
                ]),
                m,
            );
        }
        "tabNew" => {
            m.tabs.push(Tab {
                path: m.path.clone(),
                cursor: 0,
                back: Vec::new(),
                forward: Vec::new(),
            });
            let index = m.tabs.len() - 1;
            tab(m, index, w)?;
        }
        "openTerminal" | "windowNew" => {
            let child = super::terminal::launch(&m.path, action == "windowNew")?;
            m.launches.push((if action == "windowNew" { "New Flea window" } else { "Terminal" }.into(), child));
        }
        "tabClose" => {
            if m.tabs.len() == 1 {
                m.quit = true;
            } else {
                m.tabs.remove(m.tab);
                m.tab = m.tab.min(m.tabs.len() - 1);
                m.restore_cursor = Some(m.tabs[m.tab].cursor);
                m.back = m.tabs[m.tab].back.clone();
                m.forward = m.tabs[m.tab].forward.clone();
                let p = m.tabs[m.tab].path.clone();
                m.open(p, w)?;
            }
        }
        "preview" => {
            m.quicklook = true;
            super::preview::load(m, true);
        }
        "togglePreview" => {
            m.preview_visible = !m.preview_visible;
            m.preview_focus = false;
            save_preview_column(m);
        }
        "loadPreview" => super::preview::load(m, true),
        "focusPreview" | "focusNext" => m.preview_focus = m.preview_visible,
        "tabNext" => return tab(m, (m.tab + 1) % m.tabs.len(), w),
        "tabPrevious" => return tab(m, (m.tab + m.tabs.len() - 1) % m.tabs.len(), w),
        "menu" => {
            m.menu = true;
            m.menu_cursor = if m.rows.contains_key(&m.cursor) { 0 } else { 1 };
            m.menu_top = 0;
            m.taildrop.submenu = false;
            m.taildrop.refresh();
            m.menu_ready = false;
            m.menu_action.clear();
            m.menu_path = m.current_path().unwrap_or_default();
            m.menu_directory = m.rows.get(&m.cursor).is_some_and(|row| row.directory);
            let mut rows: Vec<usize> = if m.selected.is_empty() { m.rows.contains_key(&m.cursor).then_some(m.cursor).into_iter().collect() } else { m.selected.iter().copied().collect() };
            m.menu_count = rows.len();
            if !m.menu_path.as_os_str().is_empty() && !rows.contains(&m.cursor) { rows.push(m.cursor); }
            if !rows.is_empty() {
                m.action_id = m.action_id.wrapping_add(1).max(1);
                m.menu_action = "menu".into();
                w.send(vec![("c", word("menuaction")), ("op", word("snapshot")), ("id", super::wire::number(m.action_id)), ("rows", Json::Arr(rows.into_iter().map(super::wire::number).collect()))])?;
            }
        }
        "keymapSheet" => {
            m.properties = None;
            m.sheet = true;
            m.sheet_top = 0;
        }
        "properties" => {
            if m.selected.len() > 1 {
                m.fail("Properties requires one selected item".into());
            } else if m.current_path().is_some() {
                m.action_id = m.action_id.wrapping_add(1).max(1);
                m.sheet = true;
                m.sheet_top = 0;
                m.properties = Some(vec!["Loading properties…".into()]);
                w.send(vec![("c", word("menuaction")), ("op", word("snapshot")), ("id", super::wire::number(m.action_id)), ("rows", m.indices())])?;
            }
        }
        "reveal" => {
            if !m.search.is_empty() {
                m.restore_path = m.current_path();
                if let Some(p) = m
                    .current_path()
                    .and_then(|p| p.parent().map(|v| v.to_path_buf()))
                {
                    navigate(m, p, w)?;
                }
            }
        }
        "escape" => {
            if let Some(batch) = &mut m.bulk {
                batch.cancelled = true;
                return Ok(());
            }
            if matches!(m.menu_action.as_str(), "deleteRunning" | "deleteCancelling") {
                if m.menu_action == "deleteRunning" {
                    m.menu_action = "deleteCancelling".into();
                    w.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", super::wire::number(m.action_id))])?;
                }
                return Ok(());
            }
            m.menu_action.clear();
            m.restore_selection.clear();
            m.restore_marks.clear();
            m.delete_marks.clear();
            if m.taildrop_target.take().is_some() { m.message.clear(); }
            if !m.error.is_empty() {
                m.dismiss_error();
            } else if !m.filter.is_empty() {
                m.apply_filter(String::new());
            } else if m.searching {
                w.send(vec![("c", word("searchcancel"))])?;
            } else if !m.search.is_empty() {
                let path = m.search_from.clone().unwrap_or_else(|| m.path.clone());
                m.open(path, w)?;
            } else if m.transfer_id > 0 {
                w.send(vec![
                    ("c", word("transfercancel")),
                    ("id", super::wire::number(m.transfer_id)),
                ])?;
            } else {
                m.selected.clear();
            }
        }
        "quit" => m.quit = true,
        _ => {}
    }
    m.remember_selection();
    Ok(())
}
fn deletion_key(m: &mut Model, key: &Key, wire: &mut Wire) -> io::Result<()> {
    let rows = super::render::deletion_rows(m);
    let scroll = m.deletion.as_ref().unwrap().scroll.min(rows.len().saturating_sub(1));
    let (x, y, width, count) = super::render::overlay_rect(&rows[scroll..], m.columns, m.height + super::render::CHROME_ROWS);
    let mut activate = false;
    let mut cancel = key.name == "Escape";
    if let Some(pointer) = &key.pointer {
        if pointer.released { return Ok(()); }
        let inside = pointer.x > x && pointer.x <= x + width + 2 && pointer.y > y && pointer.y <= y + count + 2;
        if matches!(pointer.button, 64 | 65) {
            let deletion = m.deletion.as_mut().unwrap();
            deletion.scroll = if pointer.button == 64 { scroll.saturating_sub(1) }
                else { (scroll + 1).min(rows.len().saturating_sub(count.max(1))) };
        } else if !inside && pointer.button == 0 && !pointer.motion { cancel = true; }
        else if inside {
            if let Some((destructive, _, _)) = super::render::deletion_buttons(m).into_iter().find(|(_, x, y)| pointer.y == *y && pointer.x >= *x && pointer.x < *x + 10) {
                if pointer.motion || pointer.button == 0 {
                let deletion = m.deletion.as_mut().unwrap();
                if destructive && deletion.token == 0 { return Ok(()); }
                deletion.destructive = destructive && deletion.token != 0;
                activate = pointer.button == 0 && !pointer.motion;
                }
            }
        }
    } else {
        let deletion = m.deletion.as_mut().unwrap();
        if key.mods.is_empty() && key.name == "Up" { deletion.scroll = scroll.saturating_sub(1); }
        else if key.mods.is_empty() && key.name == "Down" { deletion.scroll = (scroll + 1).min(rows.len().saturating_sub(count.max(1))); }
        else if (matches!(key.name.as_str(), "Tab" | "Backtab") && matches!(key.mods.as_str(), "" | "shift"))
            || (key.mods.is_empty() && (matches!(key.name.as_str(), "Right" | "Left") || matches!(key.text.as_str(), "h" | "l"))) {
            deletion.destructive = deletion.token != 0 && match key.name.as_str() {
                "Left" => false, "Right" => true,
                _ if key.text == "h" => false, _ if key.text == "l" => true,
                _ => !deletion.destructive,
            };
            deletion.scroll = rows.len().saturating_sub(count.max(1));
        } else if key.mods.is_empty() && matches!(key.name.as_str(), "Return" | "Enter" | "Space") { activate = true; }
    }
    let deletion = m.deletion.as_ref().unwrap();
    if cancel || (activate && !deletion.destructive) {
        m.deletion = None;
        m.menu_action.clear();
        wire.send(vec![("c", word("menuaction")), ("op", word("close")), ("id", super::wire::number(m.action_id))])?;
    } else if activate && deletion.token != 0 && super::render::deletion_buttons(m).iter().any(|(destructive, _, _)| *destructive) {
        let token = deletion.token;
        m.deletion = None;
        m.menu_action = "deleteRunning".into();
        wire.send(vec![("c", word("menuaction")), ("op", word("delete")), ("id", super::wire::number(m.action_id)), ("token", super::wire::number(token))])?;
    }
    Ok(())
}
fn pointer_key(
    m: &mut Model,
    key: &Key,
    pointer: &super::input::Pointer,
    map: &Map,
    w: &mut Wire,
) -> io::Result<()> {
    if pointer.released {
        m.drag_anchor = None;
        return Ok(());
    }
    let scroll = match pointer.button {
        64 => -1,
        65 => 1,
        _ => 0,
    };
    if m.editor.is_some() {
        return Ok(());
    }
    if m.sheet || m.menu {
        let rows = if m.sheet {
            super::render::panel_rows(m, map).into_iter().skip(m.sheet_top).collect()
        } else {
            super::render::menu_rows(m)
        };
        let (x, y, width, count) = super::render::overlay_rect(&rows, m.columns, m.height + super::render::CHROME_ROWS);
        if scroll != 0 {
            return self::key(
                m,
                &Key::named(if scroll < 0 { "Up" } else { "Down" }, ""),
                map,
                w,
            );
        }
        let inside = pointer.x > x + 1
            && pointer.x <= x + width + 1
            && pointer.y > y + 1
            && pointer.y <= y + count + 1;
        if inside && m.menu {
            let index = m.menu_top + pointer.y - y - 2;
            if m.menu_enabled(index) && (pointer.motion || pointer.button == 0) {
                m.menu_cursor = index;
                if !pointer.motion {
                    return self::key(m, &Key::named("Return", ""), map, w);
                }
            }
        } else if !inside && !pointer.motion && pointer.button == 0 {
            m.menu = false;
            m.menu_action.clear();
            m.sheet = false;
        }
        return Ok(());
    }
    if pointer.motion && pointer.button == 3 { return Ok(()); }
    let (left, middle, _) = super::render::panes(m.columns, m.preview_visible);
    if !m.quicklook && pointer.y == 1 && pointer.button == 0 && !pointer.motion {
        let mut x = 2;
        for (i, label) in super::render::tab_layout(m, m.columns.saturating_sub(2)) {
            let width = super::render::text_width(&label);
            if pointer.x >= x && pointer.x < x + width {
                return tab(m, i, w);
            }
            x += width;
        }
        return Ok(());
    }
    if pointer.y < super::render::BODY_ROW || pointer.y >= m.height + super::render::BODY_ROW {
        return Ok(());
    }
    if m.quicklook || (m.preview_visible && pointer.x > left + middle + 2) {
        m.preview_focus = true;
        if scroll != 0 {
            return self::key(
                m,
                &Key::named(if scroll < 0 { "Up" } else { "Down" }, ""),
                map,
                w,
            );
        }
        let (start, columns) = super::render::preview_area(m.columns, m.quicklook);
        if pointer.button == 0 && pointer.y == super::render::BODY_ROW + m.height.saturating_sub(2)
            && pointer.x > start && pointer.x <= start + columns
        {
            let cell = pointer.x - start - 1;
            if let Some(pdf) = &mut m.pdf {
                if let Some((control, activate)) = pdf.control_at(cell, m.quicklook, columns) {
                    pdf.control = control;
                    if activate && !pointer.motion {
                        return self::key(m, &Key::named("Return", ""), map, w);
                    }
                }
            } else if let Some(player) = &mut m.player {
                if cell < 7 && !pointer.motion {
                    player.control = 0;
                    return player.toggle();
                }
                player.control = 1;
                return player.seek_at(cell, columns);
            }
        }
        return Ok(());
    }
    if m.pending.is_some() {
        return Ok(());
    }
    if scroll != 0 {
        m.preview_focus = false;
        return m.move_by(scroll, false, w);
    }
    if pointer.x <= left {
        if pointer.button == 0 && !pointer.motion {
            if let Some(row) = m.parents.get(pointer.y - super::render::BODY_ROW) {
                let path = m.path.parent().unwrap_or(&m.path).join(&row.name);
                if row.directory {
                    navigate(m, path, w)?;
                }
            }
        }
        return Ok(());
    }
    if pointer.x == left + 1 || pointer.x > left + middle + 1 {
        return Ok(());
    }
    let index = if m.filter.is_empty() {
        m.top + pointer.y - super::render::BODY_ROW
    } else {
        m.shown().get(pointer.y - super::render::BODY_ROW).copied().unwrap_or(usize::MAX)
    };
    if !m.rows.contains_key(&index) {
        return Ok(());
    }
    m.preview_focus = false;
    if pointer.motion {
        if let Some(anchor) = m.drag_anchor {
            m.selected = if m.filter.is_empty() {
                (anchor.min(index)..=anchor.max(index)).collect()
            } else {
                m.shown()
                    .into_iter()
                    .filter(|i| *i >= anchor.min(index) && *i <= anchor.max(index))
                    .collect()
            };
            m.cursor = index;
            m.remember_selection();
        }
        return Ok(());
    }
    if pointer.button == 2 {
        m.cursor = index;
        if !m.selected.contains(&index) {
            m.selected.clear();
        }
        return act(m, "menu", w);
    }
    if pointer.button != 0 {
        return Ok(());
    }
    let old = m.cursor;
    m.cursor = index;
    if key.mods == "ctrl" || key.mods == "super" {
        if !m.selected.remove(&index) {
            m.selected.insert(index);
        }
    } else if key.mods == "shift" {
        m.selected = if m.filter.is_empty() {
            (old.min(index)..=old.max(index)).collect()
        } else {
            m.shown()
                .into_iter()
                .filter(|i| *i >= old.min(index) && *i <= old.max(index))
                .collect()
        };
    } else if key.mods.is_empty() {
        m.selected.clear();
        m.drag_anchor = Some(index);
        if let Some(path) = m.current_path() {
            const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);
            if m.last_click
                .as_ref()
                .is_some_and(|(previous, at)| previous == &path && at.elapsed() <= DOUBLE_CLICK)
            {
                m.last_click = None;
                return act(
                    m,
                    if m.search.is_empty() {
                        "open"
                    } else {
                        "reveal"
                    },
                    w,
                );
            }
            m.last_click = Some((path, std::time::Instant::now()));
        }
    }
    m.remember_selection();
    Ok(())
}
pub(super) fn navigate(m: &mut Model, path: PathBuf, w: &mut Wire) -> io::Result<()> {
    if m.pending.is_some() {
        return Ok(());
    }
    m.navigation_before = Some((m.back.clone(), m.forward.clone(), m.tab));
    m.back.push(m.path.clone());
    m.forward.clear();
    m.open(path, w)
}
fn history(m: &mut Model, forward: bool, w: &mut Wire) -> io::Result<()> {
    if m.pending.is_some() {
        return Ok(());
    }
    m.navigation_before = Some((m.back.clone(), m.forward.clone(), m.tab));
    let next = if forward {
        m.forward.pop()
    } else {
        m.back.pop()
    };
    if let Some(path) = next {
        if forward {
            m.back.push(m.path.clone());
        } else {
            m.forward.push(m.path.clone());
        }
        m.open(path, w)?;
    }
    Ok(())
}
fn tab(m: &mut Model, index: usize, w: &mut Wire) -> io::Result<()> {
    if m.pending.is_some() || index >= m.tabs.len() || index == m.tab {
        return Ok(());
    }
    m.navigation_before = Some((m.back.clone(), m.forward.clone(), m.tab));
    m.tabs[m.tab] = Tab {
        path: m.path.clone(),
        cursor: m.cursor,
        back: m.back.clone(),
        forward: m.forward.clone(),
    };
    m.tab = index;
    let next = m.tabs[index].clone();
    m.restore_cursor = Some(next.cursor);
    m.back = next.back;
    m.forward = next.forward;
    m.open(next.path, w)
}
fn save(key: &str, value: Json, m: &mut Model) {
    if let Err(e) =
        crate::uistore::Store::user().and_then(|s| s.update(&Json::Obj(vec![(key.into(), value)])))
    {
        m.fail(format!("Could not save settings: {}", e));
    }
}
fn save_preview_column(m: &mut Model) {
    save(
        "preview",
        Json::Obj(vec![("column".into(), Json::Bool(m.preview_visible))]),
        m,
    );
}
fn edit(m: &mut Model, key: &Key, w: &mut Wire) -> io::Result<()> {
    let mut editor = m.editor.take().unwrap();
    let kind = editor.kind;
    if editor.pending {
        m.editor = Some(editor);
        return Ok(());
    }
    if key.name == "Escape" {
        if kind == "filter" {
            m.apply_filter(String::new());
        }
        return Ok(());
    }
    if key.name == "Return" || key.name == "Enter" {
        if !editor.valid() {
            m.editor = Some(editor);
            return Ok(());
        }
        let value = &editor.value;
        match kind {
            "path" => {
                let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
                let p = super::completion::expand(value, &home, &editor.path);
                navigate(m, p, w)?;
            }
            "filter" => m.apply_filter(value.clone()),
            "search" => {
                if value.is_empty() {
                    return Ok(());
                }
                if m.search_from.is_none() {
                    m.search_from = Some(m.path.clone());
                }
                let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
                if !m.search_here && home.is_absolute() && m.path.starts_with(&home) {
                    m.path = home;
                }
                m.invalidate_rows();
                m.cursor = 0;
                m.top = 0;
                m.total = 0;
                m.searching = true;
                m.search_query = value.clone();
                m.search = "Search: starting".into();
                w.send(vec![
                    ("c", word("search")),
                    ("path", word(&m.path.to_string_lossy())),
                    ("query", word(value)),
                    ("hidden", Json::Bool(m.hidden)),
                ])?;
            }
            "rename" => {
                w.send(vec![
                    ("c", word("rename")),
                    ("path", word(&editor.path.to_string_lossy())),
                    ("to", word(value)),
                ])?;
            }
            "mkdir" | "newfile" => w.send(vec![
                ("c", word(kind)),
                ("op", word("newFile")),
                ("id", super::wire::number(m.action_id)),
                ("path", word(&editor.path.to_string_lossy())),
                ("name", word(value)),
            ])?,
            _ => {}
        }
        if matches!(kind, "rename" | "mkdir" | "newfile") {
            editor.pending = true;
            m.editor = Some(editor);
        }
        return Ok(());
    } else if key.name == "Tab" && kind == "path" {
        if !m.completion.is_empty() {
            editor.replace(m.completion.clone());
        }
    } else if key.name == "Tab" && kind == "search" {
        m.search_here = !m.search_here;
    } else {
        editor.update(key);
    }
    if kind == "filter" {
        m.apply_filter(editor.value.clone());
    }
    if kind == "path" {
        m.completion = m.completer.request(&editor.value, &editor.path);
    }
    m.editor = Some(editor);
    Ok(())
}
