//! The rename family: F2, the mac Return and r open the inline editor on the cursor row, or the bulk card over several rows.
use super::Order;
use crate::{state::Shell, window::startup::View};
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
    if event.context != flea::keys::Context::Listing || event.action != "rename" {
        return false;
    }
    let side = shell.focus_side;
    let several = shell.panes[side]
        .as_ref()
        .is_some_and(|host| host.selected.len() > 1);
    // The inline editor is drawn over the list rows; Grid and Columns draw theirs elsewhere.
    if !several && !matches!(shell.view, View::List | View::Dual) {
        return false;
    }
    let ran = shell.rename_open(side, 0, window, cx);
    shell.rename_sync_gate(window, cx);
    ran
}
