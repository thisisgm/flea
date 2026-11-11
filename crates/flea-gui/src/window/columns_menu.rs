//! Neighbour menus retain their drawn directory and row without navigating the pane.
use crate::{
    columns::menu::menu_on_neighbour, g2::context_menus::target::PathMenu, state::Shell,
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
}
