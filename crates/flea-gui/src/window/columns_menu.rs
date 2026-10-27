//! A neighbour row becomes the active listing before its selected row owns a menu, as ColumnsArea.qml does.
use crate::{columns::menu::menu_on_neighbour, state::Shell, window::navigate::Step};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn columns_neighbour_menu(
        &mut self,
        side: usize,
        base: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = menu_on_neighbour(base, name);
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        host.nav.pending_select = Some(target.select.clone());
        host.nav.menus.row_menu = true;
        if let Some(columns) = host.columns.as_mut() {
            columns.row_menu = Some(target);
        }
        if self.step(side, Step::OpenPlace(base.to_owned()), window, cx) != Ok(true) {
            if let Some(host) = self.panes[side].as_mut() {
                host.nav.menus.row_menu = false;
                host.nav.pending_select = None;
                if let Some(columns) = host.columns.as_mut() {
                    columns.row_menu = None;
                }
            }
        }
    }

    pub(crate) fn columns_row_menu_landed(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        let Some(columns) = host.columns.as_mut() else {
            return;
        };
        if !host.nav.menus.row_menu {
            columns.row_menu = None;
            return;
        }
        if host.pane.busy() {
            return;
        }
        host.nav.menus.row_menu = false;
        let Some(target) = columns.row_menu.take() else {
            return;
        };
        let landed = host.nav.landed == Some((host.pane.held.listing, host.pane.cursor));
        let own_row = host.pane.held.row(host.pane.cursor).is_some_and(|row| {
            crate::navigation::history::join(&host.path, &row.name) == target.select
        });
        if landed && own_row && host.path == target.open {
            self.menu_keyboard_open(window, cx);
        }
    }
}
