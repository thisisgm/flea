//! The listing family: "/" and the keys a standing filter owns on the focused pane.
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
    shell.filter_action(event.action, window, cx)
}
