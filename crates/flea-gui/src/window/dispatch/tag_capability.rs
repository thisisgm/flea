//! The pane's accepted filesystem capability gates every Tags entry path.
use super::Order;
use crate::state::Shell;
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn can_tag(&self, side: usize) -> bool {
        self.surfaces
            .get(side)
            .is_none_or(|surface| surface.filesystem.can_tag())
    }

    pub(crate) fn refresh_tag_capability(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_opened()
            && self
                .menu
                .captured
                .as_ref()
                .is_some_and(|capture| capture.side == side)
        {
            let entries = self.menu_entries();
            let list = self.panes[side].as_ref().unwrap().list.borrow();
            self.menu
                .menu
                .borrow_mut()
                .refresh(entries, &self.menu_env(&list.typeface), window);
            cx.notify();
        }
    }
}

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let side = match order {
        Order::Action(event)
            if event.context == flea::keys::Context::Listing && event.action == "tags" =>
        {
            shell.focus_side
        }
        Order::Menu(chosen) if chosen.action == "tags" => chosen.captured.side,
        _ => return false,
    };
    if shell.can_tag(side) {
        return false;
    }
    if let Order::Menu(chosen) = order {
        if !chosen.claim() {
            return true;
        }
        shell.retire_menu(window, cx);
    }
    shell.menu_notice(side, shell.locale.catalog.tr("places.tag.unsupported"), cx);
    true
}
