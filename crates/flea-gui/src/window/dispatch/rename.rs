//! The rename family: F2, the mac Return and r open the inline editor on the cursor row, or the bulk card over several rows.
use super::Order;
use crate::state::Shell;
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
    let ran = shell.rename_open(side, 0, window, cx);
    shell.rename_sync_gate(window, cx);
    ran
}
