//! Neighbour menus retain their drawn directory and row without navigating the pane.
use crate::{
    columns::menu::menu_on_neighbour, g2::mn1::target::PathMenu, state::Shell,
    window::navigate::Step,
};
use gpui::{Context, Window};

impl Shell {
    pub(crate) fn columns_neighbour_menu(
        &mut self,
        side: usize,
        target: PathMenu,
        at: (f32, f32),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.begin_menu(
            side,
            target.row.as_ref().map(|_| 0),
            at,
            (false, Some(target)),
            window,
            cx,
        );
    }
    /// A neighbour file's one tap: its folder becomes the listing with the cursor on it, as Finder selects any row.
    pub(crate) fn columns_neighbour_file(
        &mut self,
        side: usize,
        base: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = menu_on_neighbour(base, name).select;
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        host.nav.pending_select = Some(target.clone());
        if self.step(side, Step::OpenPlace(base.to_owned()), window, cx) != Ok(true) {
            if let Some(host) = self.panes[side].as_mut() {
                host.nav.pending_select = None;
            }
            return;
        }
        if let Some(columns) = self.panes[side]
            .as_mut()
            .and_then(|host| host.columns.as_mut())
        {
            columns.file_tap = Some(target);
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
