//! Header.qml's column menu uses the shared card and the existing Settings column writer.
use crate::{
    list::columns::{Column, Switched},
    state::Shell,
    window::startup::View,
};
use flea::{
    ui_logic_mn1::{rows::header_entries, Entry},
    ui_logic_st1::{edit, pages},
};
use gpui::{Context, MouseDownEvent, Window};

impl Shell {
    pub(crate) fn header_menu_entries(&self, side: usize) -> Vec<Entry> {
        let shown = Switched::from_settings(&self.launch.settings.data);
        let hidden = Column::ALL
            .into_iter()
            .filter(|&column| !shown.on(column))
            .map(|column| column.key().to_owned())
            .collect::<Vec<_>>();
        let hidden_files = self.panes[side]
            .as_ref()
            .is_some_and(|host| host.nav.folders.borrow().hidden);
        header_entries(&hidden, hidden_files)
    }

    pub(crate) fn header_menu_open(
        &mut self,
        side: usize,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.input_blocked(cx)
            || !matches!(self.shown_view(), View::List | View::Dual)
            || self.panes[side]
                .as_ref()
                .is_none_or(|host| host.stopped || host.pane.state.search)
        {
            return;
        }
        self.open_menu_with(
            side,
            None,
            (event.position.x.into(), event.position.y.into()),
            true,
            window,
            cx,
        );
        cx.stop_propagation();
    }

    pub(crate) fn header_menu_choose(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.menu.header {
            return false;
        }
        let Some(key) = action.strip_prefix("col:") else {
            return false;
        };
        let Some(row) = pages::column_rows(&self.launch.settings.data)
            .into_iter()
            .find(|row| row.id == format!("column:{key}"))
        else {
            return false;
        };
        let patch = edit::activate("columns", &row, &self.launch.settings.data, 14);
        self.retire_menu(window, cx);
        if let edit::Edit::Patch(patch) = patch {
            self.change_settings(patch, window, cx);
        }
        true
    }
}
