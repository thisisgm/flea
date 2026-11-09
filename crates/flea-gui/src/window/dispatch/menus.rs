//! The menus family: the context menu rows whose command lives in the window itself, from ui/PaneMenuActions.qml perform.
use super::Order;
use crate::{g2::mn1::join::Chosen, state::Shell};
use flea::{jsondoc::Json, ui_logic_st1::edit};
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    // PaneMenuActions.qml open: the listing's Properties key opens the card the menu row opens, with no menu drawn.
    if let Order::Action(event) = order {
        return event.context == flea::keys::Context::Listing
            && event.action == "properties"
            && shell.properties_listing(window, cx);
    }
    let Order::Menu(chosen) = order else {
        return false;
    };
    let wanted = match chosen.action.as_str() {
        "compare" => {
            chosen.captured.ready && !chosen.captured.background && chosen.paths.len() == 2
        }
        "properties" => {
            chosen.captured.ready && !chosen.captured.background && !chosen.captured.rows.is_empty()
        }
        // Menu.js OPEN_WITH_OTHER opens the Open With card on the one row; any other leaf launches on every captured row.
        "openWith" => {
            !chosen.leaf.is_empty()
                && chosen.captured.ready
                && !chosen.captured.background
                && (chosen.captured.rows.len() == 1
                    || (!chosen.captured.rows.is_empty()
                        && chosen.leaf != flea::ui_logic_mn1::rows::OPEN_WITH_OTHER))
        }
        // The search results' own reveal: one captured row, taken to its folder the way o and Enter do.
        "goToLocation" => {
            chosen.captured.ready
                && !chosen.captured.background
                && !chosen.captured.rows.is_empty()
        }
        "selectAll" | "invertSelection" | "toggleHidden" | "toggleLangMarks" | "settings"
        | "updateFlea" | "extThumbs" | "addFavourite" => true,
        _ => false,
    };
    if !wanted || !chosen.claim() {
        return false;
    }
    // PaneMenuActions.qml: an application leaf launches on the menu's own snapshot, which closes on its answer.
    if chosen.action == "openWith" && chosen.leaf != flea::ui_logic_mn1::rows::OPEN_WITH_OTHER {
        shell.retire_menu(window, cx);
        shell.open_with_application(chosen.captured.side, chosen.captured.id, &chosen.leaf, cx);
        return true;
    }
    finish(shell, chosen, window, cx);
    match chosen.action.as_str() {
        // The menu's validated pair opens the Compare card, as Shell::open_compare does for held rows.
        "compare" => {
            let side = chosen.captured.side;
            let mark = shell.panes[side]
                .as_ref()
                .and_then(|host| {
                    chosen
                        .captured
                        .rows
                        .first()
                        .and_then(|&row| chosen.captured.row(host, row).cloned())
                })
                .unwrap_or_else(|| flea::protocol::RowFacts {
                    mode: chosen.captured.kinds.first().copied().unwrap_or(0),
                    ..flea::protocol::RowFacts::default()
                });
            shell.open_compare_paths(
                side,
                chosen.paths[0].clone(),
                chosen.paths[1].clone(),
                &mark,
                window,
                cx,
            );
        }
        "properties" => shell.open_properties_card(&chosen.captured, window, cx),
        "openWith" => {
            if !shell.open_with_captured(&chosen.captured, cx) {
                let refusal = shell.locale.catalog.tr("dialogs.file.open.unlisted");
                shell.menu_notice(chosen.captured.side, refusal, cx);
            }
        }
        // The search results' own reveal: the row the menu captured, the same step o and Enter make.
        "goToLocation" => {
            let side = chosen.captured.side;
            let focused = std::mem::replace(&mut shell.focus_side, side);
            if let Some(&row) = chosen.captured.rows.first() {
                if let Some(host) = shell.panes[side].as_mut() {
                    host.pane.cursor = row;
                }
            }
            shell.search_action("reveal", window, cx);
            shell.focus_side = focused;
        }
        // Marks.js selectAll and invert: the rows the listing draws, so a filter narrows them.
        "selectAll" | "invertSelection" => {
            // input_action reads the focused pane, so it points at the menu's own pane for this one call.
            let focused = std::mem::replace(&mut shell.focus_side, chosen.captured.side);
            shell.input_action(&chosen.action, window, cx);
            shell.focus_side = focused;
        }
        // Pane.qml toggleHidden: the . key's own settings write, so the relist is a fresh listing as the key's is.
        "toggleHidden" => {
            let focused = std::mem::replace(&mut shell.focus_side, chosen.captured.side);
            shell.settings_action("toggleHidden", window, cx);
            shell.focus_side = focused;
        }
        // ViewState.qml toggles langMarks, which every view reads from ui.json.
        "toggleLangMarks" => toggle_lang_marks(shell, window, cx),
        "settings" => shell.open_settings(window, cx),
        // Focus.js updateFlea: the same run the About page's Update Flea starts.
        "updateFlea" => shell.settings_update(true, window, cx),
        "extThumbs" => super::window::toggle_ext_thumbs(shell, chosen.captured.side, window, cx),
        // PaneMenuActions.qml: the background adds its folder, a folder row the one path its snapshot names.
        _ => {
            let path = if chosen.captured.background {
                Some(chosen.captured.path.clone())
            } else {
                (chosen.paths.len() == 1).then(|| chosen.paths[0].clone())
            };
            match path {
                Some(path) => shell.add_favourite(&path, window, cx),
                None => shell.menu_notice(
                    chosen.captured.side,
                    shell.locale.catalog.tr("menu.refusal.favourite_unread"),
                    cx,
                ),
            }
        }
    }
    true
}

// A claimed row closes its snapshot and hands the keyboard back to the listing, as every other choice does.
fn finish(shell: &mut Shell, chosen: &Chosen, window: &mut Window, cx: &mut Context<Shell>) {
    if !chosen.captured.background {
        shell.menu_send(
            chosen.captured.side,
            format!(
                "{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{}}}",
                chosen.captured.id
            ),
            cx,
        );
    }
    shell.retire_menu(window, cx);
}

// ViewState.qml toggles langMarks, which every view reads from ui.json; the menu row and the listing key share it.
pub(super) fn toggle_lang_marks(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    let shown = shell
        .launch
        .settings
        .data
        .get("langMarks")
        .and_then(Json::as_bool)
        != Some(false);
    shell.change_settings(edit::setting("langMarks", Json::Bool(!shown)), window, cx);
}
