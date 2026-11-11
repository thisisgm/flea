//! Only an explicit menu action may adopt a neighbour's listing.
use super::join::Captured;
use crate::{state::Shell, window::navigate::Step};
use gpui::{Context, Window};

pub(crate) struct AfterNavigation {
    side: usize,
    path: String,
    action: String,
    target: Option<String>,
}

impl Shell {
    pub(super) fn neighbour_selection(
        &mut self,
        capture: &Captured,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(action, "selectAll" | "invertSelection")
            || !self.panes[capture.side]
                .as_ref()
                .is_some_and(|host| capture.external(host))
        {
            return false;
        }
        self.navigate_menu_action(capture.side, capture.path.clone(), action, None, window, cx);
        true
    }

    pub(crate) fn navigate_menu_action(
        &mut self,
        side: usize,
        path: String,
        action: &str,
        target: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retire_menu(window, cx);
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let previous = std::mem::replace(&mut host.nav.pending_select, target.clone());
        if self.step(side, Step::OpenPlace(path.clone()), window, cx) == Ok(true) {
            self.menu.after_navigation = Some(AfterNavigation {
                side,
                path,
                action: action.into(),
                target,
            });
        } else if let Some(host) = self.panes[side].as_mut() {
            host.nav.pending_select = previous;
        }
    }

    pub(crate) fn menu_target_landed(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.menu.after_navigation.as_ref() else {
            return;
        };
        if pending.side != side {
            return;
        }
        let Some(host) = self.panes[side].as_ref() else {
            self.menu.after_navigation = None;
            return;
        };
        if host.pane.place() != pending.path || host.stopped {
            self.menu.after_navigation = None;
            return;
        }
        if host.pane.busy() {
            return;
        }
        let pending = self.menu.after_navigation.take().unwrap();
        if pending.target.as_ref().is_some_and(|target| {
            host.pane.held.row(host.pane.cursor).is_none_or(|row| {
                row.dir || crate::navigation::history::join(host.pane.path(), &row.name) != *target
            })
        }) {
            self.menu_notice(side, self.locale.catalog.tr("menu.refusal.moved"), cx);
            return;
        }
        let focused = std::mem::replace(&mut self.focus_side, side);
        let dispatched = match pending.action.as_str() {
            "preview" => Some("preview"),
            "open" => Some("open"),
            _ => None,
        };
        if let Some(action) = dispatched {
            self.dispatch(
                crate::window::dispatch::Order::Action(crate::window::input::ActionEvent::bare(
                    action,
                    flea::keys::Context::Listing,
                )),
                window,
                cx,
            );
        } else {
            self.input_action(&pending.action, window, cx);
        }
        self.focus_side = focused;
    }
}
