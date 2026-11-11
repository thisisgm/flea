//! The navigation family: Enter, the folder keys, history and reload on the focused pane, through its navigator.
use super::Order;
use crate::{state::Shell, window::navigate::Step};
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Action(event) = order else {
        return false;
    };
    // Focus.js: a reload answers from the rail as well as the list.
    let rail_reload = event.context == flea::keys::Context::Rail && event.action == "reload";
    if event.context != flea::keys::Context::Listing && !rail_reload {
        return false;
    }
    let side = shell.focus_side;
    let Some(host) = shell.panes[side].as_ref() else {
        return false;
    };
    let busy = host.pane.busy();
    // Reload.js begin: a listing past its wait is let go, so a reload asks for its folder again.
    let waited = busy && host.waiting;
    let searching = host.search.owner.active();
    // Focus.js pageForward: a symlink drawn as a folder browses in like one.
    let folder = host
        .pane
        .held
        .row(host.pane.cursor)
        .map(|row| flea::ui_logic_picker_portal::directory(row.dir, row.mode, &row.icon));
    if event.action == "open"
        && !busy
        && folder.is_some()
        && !flea::ui_logic_listing_filter::cursor_shown(
            host.filter.shown.as_deref(),
            Some(host.pane.cursor),
        )
    {
        let text = shell.locale.catalog.tr("filter.row_hidden");
        shell.panes[side].as_mut().expect("focused pane").status = text.into();
        cx.notify();
        return true;
    }
    let step = match event.action {
        "open"
            if !busy
                && !host.selected.is_empty()
                && host.selected.as_slice() != [host.pane.cursor] =>
        {
            return false
        }
        // Nav.js openCursor: a file row belongs to the opener, and Focus.js sends pageForward on one to the preview.
        "open" | "pageForward" if !busy && folder == Some(false) => return false,
        // Focus.js pageForward: with no loaded row the key is quiet.
        "pageForward" if !busy && folder.is_none() => return true,
        "open" | "pageForward" => Step::OpenCursor,
        "parent" => Step::Up,
        "historyBack" => Step::Back,
        "historyForward" => Step::Forward,
        // Focus.js: a reload goes quiet while a walk owns the header.
        "reload" if searching => return true,
        "reload" if waited => Step::Retry,
        "reload" => Step::Reload,
        _ => return false,
    };
    // Shell::step puts a refusal's own sentence in the pane status, so its result is spent here.
    let _ = shell.step(side, step, window, cx);
    true
}
