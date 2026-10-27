//! A Places or Favorites right click uses the same menu primitive as the listing.
use super::Order;
use crate::{g2::pl4::producer::Event, state::Shell};
use flea::ui_logic_pl4::rail::{Group, Kind};
use gpui::{Context, Window};

pub(super) fn run(
    _: &mut Shell,
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
    if !matches!(entry.kind, Kind::Home | Kind::Favourite | Kind::Favorite)
        && entry.group != Group::Device
    {
        return false;
    }
    // Rail requests drain inside Shell::render, where a notify asks for no frame, so the menu opens after this draw.
    let (side, entry) = (request.side, entry.clone());
    cx.defer_in(window, move |shell, window, cx| {
        shell.open_place_menu(side, &entry, window, cx)
    });
    true
}
