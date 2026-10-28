use super::{
    editor::Editor,
    input::{Key, WHEEL_DOWN, WHEEL_UP},
    keymap::Map,
    model::{Model, Tab},
    wire::{word, Wire},
};
use crate::jsondoc::Json;
use std::{io, path::PathBuf};

pub fn key(model: &mut Model, key: &Key, map: &Map, wire: &mut Wire) -> io::Result<()> {
    let armed = std::mem::take(&mut model.key_arm);
    if model.choose_pending {
        if matches!(
            map.action(key, &model.preset).as_str(),
            "escape" | "quit" | "quitNoCwd"
        ) {
            model.choose_pending = false;
            model.chosen = None;
            model.quit = true;
        }
        return Ok(());
    }
    if model.deletion.is_some() {
        return super::render::cards::delete_key(model, key, wire);
    }
    if !model.menu && !model.menu_action.is_empty() && model.menu_action != "menu" {
        let action = map.action(key, &model.preset);
        return if matches!(action.as_str(), "escape" | "quit") {
            act(model, &action, wire)
        } else {
            Ok(())
        };
    }
    if model.prompts.question.is_some() {
        return super::prompts::answer(model, key, wire);
    }
    if model.menu {
        return super::render::menu_run::key(model, key, map, wire);
    }
    if model.sheet {
        return super::render::cards::sheet_key(model, key, map);
    }
    if super::trashview::refuses(model, key, map, &armed, wire)? {
        return Ok(());
    }
    if let Some(pointer) = &key.pointer {
        return pointer_key(model, key, pointer, map, wire);
    }
    if model.editor.is_some() {
        return edit(model, key, wire);
    }
    if find_holds(model, &map.action(key, &model.preset)) {
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
        if super::model3d::key(model, key, &action, wire)? {
            return Ok(());
        }
        if matches!(action.as_str(), "tabNext" | "tabPrevious") {
            return act(model, &action, wire);
        }
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
                        (model.preview_scroll + 1).min(super::layout::preview_scroll_limit(model))
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
    let mut action =
        if model.choosing && key.mods.is_empty() && matches!(key.name.as_str(), "Return" | "Enter")
        {
            "open".into()
        } else {
            map.action(key, &model.preset)
        };
    if let Some(prefix) = armed.strip_prefix("prefix:") {
        action = map.sequence(prefix, key, &model.preset, "listing");
        if action.is_empty() {
            if key.name != "Escape" {
                model.fail("No action for that key sequence".into());
            }
            return Ok(());
        }
    }
    if action.starts_with("prefix:") {
        model.key_arm = action;
        return Ok(());
    }
    // Issue 133: every trash key refuses on a mount gio cannot trash, before d arms and before the backend fails.
    if matches!(action.as_str(), "trash" | "trashArm") && !super::mounts::trashable(&model.path) {
        model.fail(super::mounts::no_trash(&model.hints.delete_permanently));
        return Ok(());
    }
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
                let arm = if key.text.is_empty() {
                    key.name.as_str()
                } else {
                    key.text.as_str()
                };
                model.say(super::prompts::trash_arm(model, arm));
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
pub(super) fn act(m: &mut Model, action: &str, w: &mut Wire) -> io::Result<()> {
    if find_holds(m, action) {
        return Ok(());
    }
    if !m.menu_action.is_empty()
        && m.menu_action != "menu"
        && !matches!(action, "quit" | "quitNoCwd" | "escape")
    {
        return Ok(());
    }
    if (m.pending.is_some() || m.held || m.name_filter.waiting)
        && !matches!(action, "quit" | "quitNoCwd" | "escape" | "keymapSheet")
    {
        return Ok(());
    }
    if super::contents::guarded(m)
        && matches!(
            action,
            "open"
                | "pageForward"
                | "duplicate"
                | "copy"
                | "cut"
                | "trash"
                | "trashArm"
                | "deletePermanently"
                | "rename"
                | "preview"
                | "reveal"
                | "editFile"
                | "menu"
                | "properties"
                | "factsToggle"
                | "toggleSelect"
                | "selectNext"
                | "selectAll"
                | "invertSelection"
        )
    {
        m.fail(m.prompts.i18n.tr("search.contents.identity"));
        return Ok(());
    }
    if m.searching
        && matches!(
            action,
            "paste"
                | "movePaste"
                | "duplicate"
                | "trash"
                | "trashArm"
                | "deletePermanently"
                | "rename"
                | "copy"
                | "cut"
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
                | "editFile"
        )
    {
        return Ok(());
    }
    // A cursor move spends a re-sort's anchor, so neither its late reply nor a gap press pulls the cursor back.
    if matches!(
        action,
        "cursorDown"
            | "cursorUp"
            | "extendDown"
            | "extendUp"
            | "pageDown"
            | "pageUp"
            | "halfPageDown"
            | "halfPageUp"
            | "selectNext"
            | "first"
            | "cursorFirst"
            | "last"
            | "cursorLast"
    ) {
        m.sort_anchor = None;
    }
    match action {
        "cursorDown" => m.move_by(1, false, w)?,
        "cursorUp" => m.move_by(-1, false, w)?,
        "extendDown" => m.move_by(1, true, w)?,
        "extendUp" => m.move_by(-1, true, w)?,
        "pageDown" => m.move_by(m.file_capacity() as isize, false, w)?,
        "pageUp" => m.move_by(-(m.file_capacity() as isize), false, w)?,
        "halfPageDown" => m.move_by((m.file_capacity() / 2).max(1) as isize, false, w)?,
        "halfPageUp" => m.move_by(-((m.file_capacity() / 2).max(1) as isize), false, w)?,
        "first" | "cursorFirst" => {
            m.cursor = if m.filter.is_empty() {
                0
            } else {
                m.shown().first().copied().unwrap_or(m.cursor)
            };
            m.window(w)?;
        }
        "last" | "cursorLast" => {
            m.cursor = if m.filter.is_empty() {
                m.total.saturating_sub(1)
            } else {
                m.shown().last().copied().unwrap_or(m.cursor)
            };
            m.window(w)?;
        }
        "parent" => {
            if let Some(p) = m.path.parent().map(|p| p.to_path_buf()) {
                navigate(m, p, w)?;
            }
        }
        "historyBack" => history(m, false, w)?,
        "historyForward" => history(m, true, w)?,
        "open" | "pageForward" => {
            if m.choosing
                && (action == "open")
                && (!m.selected.is_empty() || m.rows.get(&m.cursor).is_some_and(|r| !r.directory))
            {
                if m.pending_clipboard || m.searching {
                    m.fail("Wait for the selection to settle before choosing".into());
                    return Ok(());
                }
                m.choose_count = m.selected.len().max(1);
                m.choose_pending = true;
                w.send(vec![("c", word("paths")), ("rows", m.indices())])?;
                return Ok(());
            }
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
        "selectNext" => {
            if m.rows.contains_key(&m.cursor) && !m.selected.remove(&m.cursor) {
                m.selected.insert(m.cursor);
            }
            m.move_by(1, false, w)?;
        }
        "clearSelection" => m.selected.clear(),
        "invertSelection" => {
            let rows = if m.filter.is_empty() {
                (0..m.total).collect()
            } else {
                m.shown()
            };
            m.selected = rows
                .into_iter()
                .filter(|i| !m.selected.contains(i))
                .collect();
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
        "search" | "searchContents" => {
            m.find.mode = action == "searchContents";
            if m.find.mode {
                m.find.session.scope = flea::ui_logic_sr1::Scope::Folder;
            }
            let folder = m.search_from.clone().unwrap_or_else(|| m.path.clone());
            m.editor = Some(Editor::new("search", String::new(), folder));
        }
        "reload" => m.refresh(w)?,
        "goHome" | "goConfig" | "goDownloads" => {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute());
            if let Some(home) = home {
                let path = match action {
                    "goConfig" => home.join(".config"),
                    "goDownloads" => home.join("Downloads"),
                    _ => home,
                };
                navigate(m, path, w)?;
            } else {
                m.fail("No home folder is configured".into());
            }
        }
        "editFile" => {
            if m.rows.get(&m.cursor).is_some_and(|r| !r.directory) {
                m.edit_file = m.current_path();
            } else {
                m.fail("Only a regular file can be edited".into());
            }
        }
        "rename" => {
            if m.selected.len() > 1 {
                m.action_id = m.action_id.wrapping_add(1).max(1);
                m.menu_action = "bulkSnapshot".into();
                m.menu_count = m.selected.len();
                w.send(vec![
                    ("c", word("menuaction")),
                    ("op", word("snapshot")),
                    ("id", super::wire::number(m.action_id)),
                    ("rows", m.indices()),
                ])?;
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
        "newFolder" => {
            m.editor = Some(Editor::named(
                "mkdir",
                m.prompts.i18n.tr("prompt.newFolder"),
                m.path.clone(),
            ))
        }
        "copy" | "cut" => {
            if m.pending_clipboard || m.taildrop_target.is_some() {
                return Ok(());
            }
            m.cut = action == "cut";
            m.pending_clipboard = true;
            w.send(vec![("c", word("paths")), ("rows", m.indices())])?;
        }
        "clearClipboard" => {
            m.clipboard.clear();
            m.cut = false;
            m.discard_clipboard |= m.pending_clipboard;
            m.say("Clipboard cleared".into());
        }
        "paste" | "movePaste" => super::prompts::paste(m, action, w)?,
        "trash" | "trashArm" => super::prompts::trash(m, w)?,
        "openTrash" => super::trashview::open(m, w)?,
        "undo" | "redo" => w.send(vec![("c", word(action))])?,
        "deletePermanently" => {
            m.action_id = m.action_id.wrapping_add(1).max(1);
            m.menu_action = "deleteSnapshot".into();
            m.deletion = Some(super::model::Deletion::default());
            m.delete_marks.clear();
            let indices: Vec<usize> = if m.selected.is_empty() {
                vec![m.cursor]
            } else {
                m.selected.iter().copied().collect()
            };
            for index in indices {
                if let Some(row) = m.selected_rows.get(&index).or_else(|| m.rows.get(&index)) {
                    m.delete_marks.insert(m.row_path(row), row.clone());
                }
            }
            w.send(vec![
                ("c", word("menuaction")),
                ("op", word("snapshot")),
                ("id", super::wire::number(m.action_id)),
                ("rows", m.indices()),
            ])?;
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
            m.editor = Some(Editor::named(
                "newfile",
                m.prompts.i18n.tr("prompt.newFile"),
                m.path.clone(),
            ));
        }
        "detailNext" => super::detail::cycle(m),
        "sortNext" | "sortReverse" | "sortName" | "sortNameReverse" | "sortDate"
        | "sortDateReverse" | "sortSize" | "sortSizeReverse" => {
            // The cursor's logical row: the loaded row, or in a listed/rows gap the anchor still waited for.
            let anchor = m.current_path().or_else(|| m.sort_anchor.clone());
            if action == "sortReverse" {
                m.reverse = !m.reverse;
            } else if action != "sortNext" {
                m.sort = if action.starts_with("sortDate") {
                    "date"
                } else if action.starts_with("sortSize") {
                    "size"
                } else {
                    "name"
                }
                .into();
                m.reverse = action.ends_with("Reverse");
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
            let mut request = vec![
                ("c", word("sort")),
                ("by", word(&m.sort)),
                ("foldersByName", Json::Bool(m.sort == "size")),
                ("desc", Json::Bool(m.reverse)),
                ("foldersFirst", Json::Bool(m.folders_first)),
                ("groupByKind", Json::Bool(m.group_by_kind)),
            ];
            if let Some(anchor) = anchor.as_ref() {
                request.push(("anchor", word(&anchor.to_string_lossy())));
            }
            w.send(request)?;
            m.sort_anchor = anchor;
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
            if tab_full(m) {
                return Ok(());
            }
            push_tab(m, m.path.clone(), w)?;
        }
        "openTab" => {
            if tab_full(m) {
                return Ok(());
            }
            match m
                .current_path()
                .filter(|_| m.rows.get(&m.cursor).is_some_and(|r| r.directory))
            {
                Some(path) => push_tab(m, path, w)?,
                None => m.say("Only a folder opens in a new tab".into()),
            }
        }
        "openTerminal" | "windowNew" => {
            let child = super::terminal::launch(&m.path, action == "windowNew")?;
            m.launches.push((
                if action == "windowNew" {
                    "New Flea window"
                } else {
                    "Terminal"
                }
                .into(),
                child,
            ));
        }
        "openAgent" => {
            let path = m.path.canonicalize()?;
            if !path.is_dir() {
                m.fail("An agent needs a folder to open in".into());
            } else {
                let child = super::launch::agent(&path)?;
                m.launches.push(("Agent".into(), child));
            }
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
        // { and } move the current tab one place, clamping at either end; it stays current and nothing re-lists.
        "tabMoveLeft" | "tabMoveRight" => {
            if m.tabs.len() > 1 {
                let from = m.tab;
                let to = (from as isize + if action == "tabMoveRight" { 1 } else { -1 })
                    .clamp(0, m.tabs.len() as isize - 1) as usize;
                if to != from {
                    let tab = m.tabs.remove(from);
                    m.tabs.insert(to, tab);
                    m.tab = to;
                }
            }
        }
        "menu" => super::render::menu_run::open(m, w)?,
        "keymapSheet" => {
            m.properties = None;
            m.sheet = true;
            m.sheet_top = 0;
            super::render::cards::probe(&m.rounded);
        }
        "properties" | "factsToggle" => {
            if m.selected.len() > 1 {
                m.fail("Properties requires one selected item".into());
            } else if m.current_path().is_some() {
                m.action_id = m.action_id.wrapping_add(1).max(1);
                m.sheet = true;
                m.sheet_top = 0;
                m.properties = Some(vec![(String::new(), m.i18n.tr("properties.loading"))]);
                super::render::cards::probe(&m.rounded);
                w.send(vec![
                    ("c", word("menuaction")),
                    ("op", word("snapshot")),
                    ("id", super::wire::number(m.action_id)),
                    ("rows", m.indices()),
                ])?;
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
                    w.send(vec![
                        ("c", word("menuaction")),
                        ("op", word("close")),
                        ("id", super::wire::number(m.action_id)),
                    ])?;
                }
                return Ok(());
            }
            m.menu_action.clear();
            m.restore_selection.clear();
            m.restore_marks.clear();
            m.delete_marks.clear();
            if m.taildrop_target.take().is_some() {
                m.message.clear();
            }
            if !m.error.is_empty() {
                m.dismiss_error();
            } else if !m.filter.is_empty() {
                m.apply_filter(String::new(), w)?;
            } else if m.searching {
                super::contents::cancel(m, w)?;
            } else if !m.search.is_empty() {
                let path = m.search_from.clone().unwrap_or_else(|| m.path.clone());
                m.open(path, w)?;
            } else if m.active_operation() > 0 {
                w.send(vec![
                    ("c", word("transfercancel")),
                    ("id", super::wire::number(m.active_operation())),
                ])?;
            } else {
                m.selected.clear();
            }
        }
        "quit" => m.quit = true,
        "quitNoCwd" => {
            m.no_cwd = true;
            m.quit = true;
        }
        _ if action.starts_with("tab")
            && action.len() == 4
            && matches!(action.as_bytes()[3], b'1'..=b'9') =>
        {
            return tab(m, (action.as_bytes()[3] - b'1') as usize, w);
        }
        _ => m.fail(super::parity::refusal(action).into()),
    }
    m.remember_selection();
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
        WHEEL_UP => -1,
        WHEEL_DOWN => 1,
        _ => 0,
    };
    if m.editor.is_some() {
        return Ok(());
    }
    if pointer.motion && pointer.button == 3 {
        return Ok(());
    }
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
    if super::contents::holding(m) {
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
        if super::look::press(m, pointer) {
            return Ok(());
        }
        let Some((row, start, columns)) = super::picture::control_row(m) else {
            return Ok(());
        };
        if pointer.button == 0
            && pointer.y == row
            && pointer.x > start
            && pointer.x <= start + columns
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
    if m.pending.is_some() || m.held || m.name_filter.waiting {
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
    let offset = (pointer.y - super::render::BODY_ROW) / super::contents::row_height(m);
    let index = if m.filter.is_empty() {
        m.top + offset
    } else {
        m.visible().get(offset).copied().unwrap_or(usize::MAX)
    };
    if !m.rows.contains_key(&index) || !super::contents::operable(m, index) {
        return Ok(());
    }
    m.preview_focus = false;
    // A click or drag on a row is the operator choosing the cursor, as a cursor key is.
    m.sort_anchor = None;
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
// Both front ends cap the strip at nine, so a tenth tab is refused with the same words; the status line takes no final period.
fn tab_full(m: &mut Model) -> bool {
    if m.tabs.len() >= crate::uischema::MAX_LAST_TABS {
        m.say("Nine tabs is the most".into());
        return true;
    }
    false
}
// Both arms push a fresh tab on the given path and switch to it.
fn push_tab(m: &mut Model, path: std::path::PathBuf, w: &mut Wire) -> io::Result<()> {
    m.tabs.push(Tab {
        path,
        cursor: 0,
        back: Vec::new(),
        forward: Vec::new(),
    });
    let index = m.tabs.len() - 1;
    tab(m, index, w)
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
    let saved = match m.store.as_ref() {
        Some(store) => store.update(&Json::Obj(vec![(key.into(), value)])),
        None => return,
    };
    if let Err(e) = saved {
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
    if key.name == "Escape"
        || (m.preset == "yazi"
            && key.mods == "ctrl"
            && matches!(key.name.as_str(), "C" | "BracketLeft"))
    {
        if kind == "filter" {
            m.apply_filter(String::new(), w)?;
        }
        if kind == "search" {
            if let Some(active) = &m.find.active {
                m.find.mode = active.contents;
                m.find.session.scope = active.scope;
            }
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
            "filter" => m.apply_filter(value.clone(), w)?,
            "search" => {
                if value.is_empty() {
                    return Ok(());
                }
                let started = super::contents::start(m, value.clone(), editor.path.clone(), w)?;
                if !started {
                    m.editor = Some(editor);
                    return Ok(());
                }
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
    } else if key.name == "G" && key.mods == "ctrl" && kind == "search" {
        m.find.mode = !m.find.mode;
    } else if key.name == "Tab" && kind == "search" {
        m.find.session.scope = m.find.session.next_scope();
    } else {
        editor.update(key);
    }
    if kind == "filter" {
        m.apply_filter(editor.value.clone(), w)?;
    }
    if kind == "path" {
        m.completion = m.completer.request(&editor.value, &editor.path);
    }
    m.editor = Some(editor);
    Ok(())
}

// A held frame has a presentation path, but only navigation, query editing and cancellation may act on it.
fn find_holds(m: &Model, action: &str) -> bool {
    super::contents::holding(m)
        && !matches!(
            action,
            "search"
                | "searchContents"
                | "escape"
                | "quit"
                | "quitNoCwd"
                | "parent"
                | "historyBack"
                | "historyForward"
                | "goHome"
                | "goConfig"
                | "goDownloads"
                | "tabNext"
                | "tabPrevious"
        )
}
pub(super) fn send_find(
    m: &Model,
    ask: &super::contents::Query,
    folder: &std::path::Path,
    count: usize,
    wire: &mut Wire,
) -> io::Result<()> {
    use flea::ui_logic_sr1::Request;
    if ask.contents {
        super::contents::send(
            wire,
            Request::Contents {
                path: ask.root.to_string_lossy().into_owned(),
                query: ask.query.clone(),
                hidden: m.hidden,
                gen: ask.gen as u64,
                count,
            },
        )
    } else {
        super::contents::send(
            wire,
            Request::Open {
                scope: ask.root.to_string_lossy().into_owned(),
                folder: folder.to_string_lossy().into_owned(),
                hidden: m.hidden,
            },
        )?;
        super::contents::send(
            wire,
            Request::Query {
                query: ask.query.clone(),
                gen: ask.gen as u64,
                count,
                sizes: 0,
            },
        )
    }
}
