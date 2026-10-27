//! Tags belongs to its captured menu selection, including a T press from the listing.
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
        Order::Action(event)
            if event.action == "tags" && event.context == flea::keys::Context::Listing =>
        {
            shell.open_tags(window, cx);
            true
        }
        Order::Menu(chosen) => shell.choose_tag(chosen, window, cx),
        _ => false,
    }
}
