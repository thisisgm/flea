//! A tag place opens the pane's existing Home-scoped search session.
use super::Order;
use crate::{g2::pl4::producer::Event, state::Shell};
use flea::ui_logic_pl4::{rail::Kind, tags};
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
    let Event::Activate(entry) = &request.event else {
        return false;
    };
    if entry.kind != Kind::Tag {
        return false;
    }
    let Some(name) = entry.tag.filter(|name| tags::index_of(name).is_some()) else {
        return true;
    };
    shell.search_tag(request.side, name, window, cx);
    true
}
