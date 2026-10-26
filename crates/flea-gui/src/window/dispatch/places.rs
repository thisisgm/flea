//! A Places or Favorites right click uses the same menu primitive as the listing.
use super::Order;
use crate::{g2::pl4::producer::Event, state::Shell};
use flea::ui_logic_pl4::rail::Kind;
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Place(request) = order else {
        return false;
    };
    let Event::Menu(entry) = &request.event else {
        return false;
    };
    if !matches!(entry.kind, Kind::Home | Kind::Favourite | Kind::Favorite) {
        return false;
    }
    shell.open_place_menu(request.side, entry, window, cx);
    true
}
