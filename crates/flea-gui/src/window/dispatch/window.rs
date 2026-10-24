//! The window family: Add a Network Share and the keys the network menu and share browser resolve.
use super::Order;
use crate::state::Shell;
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    match order {
        Order::Network(event) => shell.network_run(event.action, window, cx),
        Order::Action(event) if event.action == "addNetwork" => {
            shell.network_add(shell.focus_side, window, cx);
            true
        }
        _ => false,
    }
}
