//! Remaining listing menu commands share their action handler with direct listing dispatch.
use super::Order;
use crate::{state::Shell, window::navigate::Step};
use flea::{jsondoc::Json, keys::Context as Keys, ui_logic_st1::edit};
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    match order {
        Order::Action(event) if event.context == Keys::Listing => match event.action {
            "toggleLangMarks" => toggle(shell, window, cx),
            "showOriginal" => {
                let side = shell.focus_side;
                let path = shell.panes[side].as_ref().and_then(|host| {
                    let row = host.pane.held.row(host.pane.cursor)?;
                    row.link.as_ref()?;
                    Some(crate::navigation::history::join(
                        host.pane.path(),
                        &row.name,
                    ))
                });
                if let Some(path) = path {
                    shell.menu_original_start(side, path, window, cx);
                } else {
                    shell.menu_explain(shell.locale.catalog.tr("menu.original.needs_link"), cx);
                }
            }
            _ => return false,
        },
        Order::Menu(chosen)
            if matches!(chosen.action.as_str(), "showOriginal" | "addToShelf")
                && chosen.captured.ready
                && !chosen.paths.is_empty()
                && chosen.claim() =>
        {
            let side = chosen.captured.side;
            shell.retire_menu(window, cx);
            if chosen.action == "showOriginal" {
                shell.menu_original_start(side, chosen.paths[0].clone(), window, cx);
            } else {
                shell.shelf_add(side, chosen.paths.clone(), window, cx);
            }
        }
        _ => return false,
    }
    true
}

fn toggle(shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>) {
    let shown = shell
        .launch
        .settings
        .data
        .get("langMarks")
        .and_then(Json::as_bool)
        != Some(false);
    shell.change_settings(edit::setting("langMarks", Json::Bool(!shown)), window, cx);
}

pub(super) fn reveal(
    shell: &mut Shell,
    side: usize,
    directory: &str,
    name: &str,
    window: &mut Window,
    cx: &mut Context<Shell>,
) {
    if let Some(host) = shell.panes[side].as_mut() {
        host.nav.pending_select =
            (!name.is_empty()).then(|| crate::navigation::history::join(directory, name));
    }
    if shell.step(side, Step::OpenPlace(directory.to_owned()), window, cx) != Ok(true) {
        if let Some(host) = shell.panes[side].as_mut() {
            host.nav.pending_select = None;
        }
    }
}
