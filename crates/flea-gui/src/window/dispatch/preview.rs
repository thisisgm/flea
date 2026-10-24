//! The preview family: the selection preview's keys on the focused pane, through its host.
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
    shell.selection_preview_action(shell.focus_side, event, window, cx)
}
