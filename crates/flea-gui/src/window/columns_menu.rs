//! Neighbour menus retain their drawn directory and row without navigating the pane.
use crate::{g2::mn1::target::PathMenu, state::Shell};
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
}
