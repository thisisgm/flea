//! The navigation family: Enter, the folder keys, history and reload on the focused pane, through its navigator.
use super::Order;
use crate::{navigation::Refusal, state::Shell, window::navigate::Step};
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
    let searching = host.search.owner.active();
    let folder = host.pane.held.row(host.pane.cursor).map(|row| row.dir);
    let step = match event.action {
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
        // Reload.js begin: a reload while a listing is out says so instead of listing.
        "reload" if busy => {
            let said = shell.locale.catalog.tr(Refusal::Busy.key()).into();
            if let Some(host) = shell.panes[side].as_mut() {
                host.status = said;
            }
            cx.notify();
            return true;
        }
        "reload" => Step::Reload,
        _ => return false,
    };
    let _ = shell.step(side, step, window, cx);
    true
}
