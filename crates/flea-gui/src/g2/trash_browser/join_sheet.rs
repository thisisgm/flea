//! The keyboard sheet borrows Trash's actions and menu snapshot, then returns focus to Trash.
use crate::{g2::settings_pages::sheet::Sheet, state::Shell};
use flea::ui_logic_settings_pages::{
    sheet::{self, Decision, MenuEntry},
    Text,
};
use flea::ui_logic_trash_browser::host::{self, Item};
use gpui::{App, Context, Window};

impl Shell {
    pub(crate) fn prepare_trash_sheet(&self, sheet: &mut Sheet, cx: &App) {
        let Some(host) = self
            .trash
            .as_ref()
            .filter(|host| host.side == self.focus_side)
        else {
            return;
        };
        let view = host.pane.read(cx).view();
        if !view.opened {
            return;
        }
        sheet.rows.retain(|row| host::sheet_action(row.action));
        sheet.sources.menus = host::sheet_menu(view.selected_count(), view.total, view.busy())
            .into_iter()
            .filter_map(|item| match item {
                Item::Entry(entry) => Some(MenuEntry {
                    label: Text::Key(entry.label_key),
                    action: entry.action.into(),
                    disabled: entry.disabled || view.operation_active,
                    danger: entry.danger,
                    ..Default::default()
                }),
                Item::Separator => None,
            })
            .collect();
        if !sheet.query.is_empty() {
            let mut candidates = sheet::action_candidates(&sheet.rows);
            candidates.extend(sheet::menu_candidates(&sheet.sources.menus, |action| {
                flea::keys::presentation::hint_for(&sheet.preset, action).into()
            }));
            candidates.extend(sheet::place_candidates(&sheet.sources.rail));
            candidates.extend(sheet::recent_candidates(
                &sheet.sources.recent,
                &sheet.sources.home,
            ));
            sheet.ranked = sheet::rank(&candidates, &sheet.query, &sheet.catalog);
            sheet.cursor = sheet.cursor.min(sheet.ranked.len().saturating_sub(1));
        }
    }

    pub(crate) fn sync_trash_sheet(&self, cx: &mut Context<Self>) {
        if let Some(view) = &self.settings.sheet {
            view.update(cx, |view, cx| {
                self.prepare_trash_sheet(&mut view.sheet, cx);
                cx.notify();
            });
        }
    }

    pub(crate) fn focus_trash_after_sheet(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(host) = self
            .trash
            .as_ref()
            .filter(|host| host.side == self.focus_side)
        {
            if host.pane.read(cx).view().opened {
                let focus = host.pane.read(cx).focus.clone();
                window.focus(&focus, cx);
            }
        }
    }

    pub(crate) fn trash_sheet_decision(
        &mut self,
        decision: &Decision,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.trash_opened(cx)
            || self
                .trash
                .as_ref()
                .is_none_or(|host| host.side != self.focus_side)
        {
            return false;
        }
        match decision {
            Decision::Action { action } if host::OWN_SHEET_KEYS.contains(&action.as_str()) => {
                let pane = self.trash.as_ref().expect("opened Trash").pane.clone();
                pane.update(cx, |pane, cx| pane.action(action, false, cx));
                self.flush_trash(window, cx);
            }
            Decision::Action { action } if host::sheet_action(action) => {
                self.forward_trash_action(action, window, cx)
            }
            Decision::Menu { menu_action } | Decision::Confirm { menu_action } => {
                let pane = self.trash.as_ref().expect("opened Trash").pane.clone();
                pane.update(cx, |pane, cx| pane.menu_action(Some(menu_action), cx));
                self.flush_trash(window, cx);
            }
            _ => return false,
        }
        true
    }
}
