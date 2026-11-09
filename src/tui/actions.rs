use super::{
    editor::Editor,
    input::Key,
    keymap::Map,
    model::{Model, Tab},
    wire::{word, Wire},
};
use crate::jsondoc::Json;
use std::{io, path::PathBuf};

#[path = "actions_pointer.rs"]
mod pointer;
use pointer::pointer_key;

pub fn key(model: &mut Model, key: &Key, map: &Map, wire: &mut Wire) -> io::Result<()> {
    let armed = std::mem::take(&mut model.key_arm);
    super::editor_input::enter_once::cancel(model, key);
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
    if model.combining.card.is_some() {
        return super::combine::key(model, key, wire);
    }
    if model.converting.card.is_some() {
        return super::convert::key(model, key, wire);
    }
    if model.printing.card.is_some() {
        return super::print::key(model, key, wire);
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
        return super::render::cards::sheet_key(model, key, map, &armed);
    }
    if super::trashview::refuses(model, key, map, &armed, wire)? {
        return Ok(());
    }
    if let Some(pointer) = &key.pointer {
        return pointer_key(model, key, pointer, map, wire);
    }
    if model.editor.is_some() {
        return super::editor_input::edit(model, key, wire);
    }
    if super::contents::holding(model) && map.action(key, &model.preset) == "print" {
        return super::print::open(model, wire, false);
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
    // Leaving Trash empties the listing before its re-list arrives, so a key in between hands the keys back too.
    super::layout::release_preview_focus(model);
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
        if let Some(quit) = super::preview_keys::quit(model, key, map, &action) {
            return act(model, &quit, wire);
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
                "preview" | "playPause" => player.toggle()?,
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
            return super::preview_keys::text(model, key, map, &armed, &action, wire);
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
    if action.is_empty() && key.text == "F" && matches!(key.mods.as_str(), "" | "shift") {
        action = "convert".into();
    }
    if let Some(prefix) = armed.strip_prefix("prefix:") {
        action = map.sequence(prefix, key, &model.preset, "listing");
        if action.is_empty() {
            if key.name != "Escape" {
                model.fail_key("tui.actions.no_action_for_that_key_sequence");
            }
            return Ok(());
        }
    }
    if action.starts_with("prefix:") {
        model.key_arm = action;
        return Ok(());
    }
    // An empty folder has nothing to trash, so d arms no prompt that no second key could answer.
    if action == "trashArm" && model.total == 0 {
        return Ok(());
    }
    // Issue 133: every trash key refuses on a mount gio cannot trash, before d arms and before the backend fails.
    if matches!(action.as_str(), "trash" | "trashArm") && !super::mounts::trashable(&model.path) {
        model.fail_text(super::mounts::no_trash_text(
            &model.hints.delete_permanently,
        ));
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
                model.say_text(super::prompts::trash_arm_text(model, arm));
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
    if action == "print" && super::contents::holding(m) {
        return super::print::open(m, w, false);
    }
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
        && !matches!(
            action,
            "quit" | "quitNoCwd" | "escape" | "keymapSheet" | "suspend"
        )
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
                | "copyPath"
                | "copyUri"
                | "copydirpath"
                | "copyDirUri"
                | "copyName"
                | "copyStem"
                | "copyRelative"
                | "copyQuoted"
                | "followLink"
                | "toggleSelect"
                | "selectNext"
                | "selectAll"
                | "invertSelection"
        )
    {
        m.fail_key("search.contents.identity");
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
                | "pasteLink"
                | "pasteAbsoluteLink"
                | "pasteHardLink"
                | "copyPath"
                | "copyUri"
                | "copydirpath"
                | "copyDirUri"
                | "copyName"
                | "copyStem"
                | "copyRelative"
                | "copyQuoted"
                | "followLink"
        )
    {
        m.fail_key("tui.operation.search_busy");
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
                | "copyPath"
                | "copyUri"
                | "copyName"
                | "copyStem"
                | "copyRelative"
                | "copyQuoted"
                | "followLink"
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
    if super::parity17::act(m, action, w)? {
        return Ok(());
    }
    match action {
        "print" => super::print::open(m, w, false)?,
        "convert" => super::convert::open(m, w, false)?,
        "combine" => super::combine::open(m, w, false)?,
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
            // A chooser hands no file to another application: Right and l on a file choose it, as Return does.
            let file = m.rows.get(&m.cursor).is_some_and(|r| !r.folder());
            if m.choosing && (file || action == "open" && !m.selected.is_empty()) {
                if m.pending_clipboard || m.searching {
                    m.fail_key("tui.actions.wait_for_the_selection_to_settle_before_choosing");
                    return Ok(());
                }
                m.choose_count = m.selected.len().max(1);
                m.choose_pending = true;
                w.send(vec![("c", word("paths")), ("rows", m.indices())])?;
                return Ok(());
            }
            if let Some(path) = m.current_path() {
                if m.rows.get(&m.cursor).is_some_and(|r| r.folder()) {
                    navigate(m, path, w)?;
                } else if crate::open::open(&path.to_string_lossy()) != 0 {
                    m.fail_key("tui.model.could_not_open_selected_file");
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
                m.find.session.scope = flea::ui_logic_file_search::Scope::Folder;
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
                m.fail_key("tui.actions.no_home_folder_is_configured");
            }
        }
        "editFile" => {
            if m.rows.get(&m.cursor).is_some_and(|r| !r.folder()) {
                m.edit_file = m.current_path();
            } else {
                m.fail_key("tui.external_editor.only_a_regular_file_can_be_edited");
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
                m.fail_key("tui.actions.selected_item_is_unavailable_select_it_again_to_rename");
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
            if m.pending_clipboard || m.parity17.copy_text.is_some() || m.taildrop_target.is_some()
            {
                return Ok(());
            }
            m.cut = action == "cut";
            m.pending_clipboard = true;
            m.parity17.copy_id = m.parity17.copy_id.wrapping_add(1).max(1);
            m.parity17.copy_listing = w.listing;
            w.send(vec![
                ("c", word("paths")),
                ("rows", m.indices()),
                ("hold", Json::Bool(true)),
                ("id", super::wire::number(m.parity17.copy_id)),
            ])?;
        }
        "clearClipboard" => {
            m.clipboard.clear();
            m.parity17.copied.clear();
            m.cut = false;
            m.discard_clipboard |= m.pending_clipboard;
            m.say_key("tui.actions.clipboard_cleared");
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
                m.fail_key("tui.actions.select_one_available_item_to_duplicate");
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
        | "sortDateReverse" | "sortSize" | "sortSizeReverse" | "sortBirth" | "sortBirthReverse"
        | "sortExtension" | "sortRandom" => {
            super::sort_actions::act(m, action, w)?;
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
                .filter(|_| m.rows.get(&m.cursor).is_some_and(|r| r.folder()))
            {
                Some(path) => push_tab(m, path, w)?,
                None => m.say_key("tui.actions.only_a_folder_opens_in_a_new_tab"),
            }
        }
        "openTerminal" | "windowNew" => {
            let child = super::terminal::launch(&m.path, action == "windowNew")?;
            m.launched(
                if action == "windowNew" {
                    "tui.actions.new_flea_window"
                } else {
                    "keys.action.openTerminal"
                },
                child,
            );
        }
        "openAgent" => {
            let path = m.path.canonicalize()?;
            if !path.is_dir() {
                m.fail_key("tui.actions.an_agent_needs_a_folder_to_open_in");
            } else {
                let child = super::launch::agent(&path)?;
                m.launched("tui.actions.agent", child);
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
        "focusPreview" | "focusNext" => m.preview_focus = super::layout::preview_focusable(m),
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
                m.fail_key("tui.actions.properties_requires_one_selected_item");
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
            if super::combine::cancel(m, w)? {
                return Ok(());
            }
            if super::convert::cancel(m, w)? {
                return Ok(());
            }
            if super::print::cancel(m, w)? {
                return Ok(());
            }
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
        _ => m.fail_text(super::parity::refusal_text(action)),
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
        m.say_key("tui.actions.nine_tabs_is_the_most");
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
// The folders the other tabs show, so the backend keeps their settled sizes across this listing.
pub(super) fn other_tabs(m: &Model) -> Json {
    let paths = m.tabs.iter().enumerate().filter(|&(i, _)| i != m.tab);
    Json::Arr(
        paths
            .map(|(_, tab)| word(&tab.path.to_string_lossy()))
            .collect(),
    )
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
pub(super) fn save(key: &str, value: Json, m: &mut Model) {
    let saved = match m.store.as_ref() {
        Some(store) => store.update(&Json::Obj(vec![(key.into(), value)])),
        None => return,
    };
    if let Err(e) = saved {
        m.fail_text(super::words::args(
            "tui.actions.save_settings",
            vec![e.to_string().into()],
        ));
    }
}
fn save_preview_column(m: &mut Model) {
    save(
        "preview",
        Json::Obj(vec![("column".into(), Json::Bool(m.preview_visible))]),
        m,
    );
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
    use flea::ui_logic_file_search::Request;
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
