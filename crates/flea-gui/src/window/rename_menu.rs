//! The rename entry uses the shared menu card and its normal keyboard and pointer routing.
use crate::{
    g2::context_menus::menu::{Face, Outcome},
    state::Shell,
};
use flea::{
    keys,
    ui_logic_context_menus::{fit::Rect, Entry, Text},
};
use gpui::{ClipboardItem, Context, MouseDownEvent, Window};

impl Shell {
    pub(crate) fn rename_menu_entries(&self) -> Vec<Entry> {
        let Some((side, session)) = self.menu.text else {
            return Vec::new();
        };
        let Some(edit) = self.rename.edit.as_ref().filter(|edit| {
            edit.side == side && edit.entry.session() == Some(session) && edit.pending.is_none()
        }) else {
            return Vec::new();
        };
        let selected = !edit.entry.selected.is_empty();
        let row = |id, key, glyph, enabled: bool| Entry {
            disabled: !enabled,
            ..Entry::row(id, Text::Key(key), glyph)
        };
        vec![
            row(
                "edit:undo",
                "keys.action.undo",
                "undo",
                edit.entry.can_undo(),
            ),
            Entry::separator(),
            row("edit:cut", "menu.row.cut", "scissors", selected),
            row("edit:copy", "menu.row.copy", "copy", selected),
            row(
                "edit:paste",
                "menu.row.paste",
                "clipboard",
                self.menu.text_paste,
            ),
            Entry::separator(),
            row(
                "edit:selectAll",
                "menu.row.select_all",
                "checkmark",
                !edit.entry.text.is_empty(),
            ),
        ]
    }

    /// Opens the text menu with the selection frozen at the point the menu was requested.
    pub(crate) fn rename_menu_open(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_opened() {
            return;
        }
        let Some(edit) = self
            .rename
            .edit
            .as_mut()
            .filter(|edit| edit.pending.is_none())
        else {
            return;
        };
        let Some(session) = edit.entry.session() else {
            return;
        };
        let side = edit.side;
        edit.line.end_selection_drag();
        self.retire_menu(window, cx);
        self.observe_menu(window, cx);
        self.menu.text = Some((side, session));
        self.menu.text_paste = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .is_some_and(|text| !text.is_empty());
        let entries = self.rename_menu_entries();
        let typeface = self.panes[side]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .typeface
            .clone();
        let env = self.menu_env(&typeface);
        let size = window.viewport_size();
        let at = (event.position.x.into(), event.position.y.into());
        self.menu.menu.borrow_mut().open(
            Face::Text,
            entries,
            at,
            Rect {
                x: 0.,
                y: 0.,
                w: size.width.into(),
                h: size.height.into(),
            },
            "",
            &env,
            window,
        );
        self.menu.pointer = Some(at);
        self.menu.resting = Some(at);
        self.menu.pointer_armed = false;
        self.keyboard.context = keys::Context::Menu;
        self.keyboard.disarm();
        cx.notify();
    }

    pub(crate) fn rename_menu_outcome(
        &mut self,
        outcome: &Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((side, session)) = self.menu.text else {
            return false;
        };
        if let Outcome::Fire { action, .. } = outcome {
            let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
            if let Some(edit) = self.rename.edit.as_mut().filter(|edit| {
                edit.side == side && edit.entry.session() == Some(session) && edit.pending.is_none()
            }) {
                let entry = &mut edit.entry;
                match action.as_str() {
                    "edit:undo" => entry.undo(),
                    "edit:selectAll" => entry.select_all(),
                    "edit:copy" | "edit:cut" if !entry.selected.is_empty() => {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            entry.text[entry.selected.clone()].to_owned(),
                        ));
                        if action == "edit:cut" {
                            entry.replace(None, "");
                        }
                    }
                    "edit:paste" => {
                        if let Some(text) = clipboard {
                            let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                            entry.replace(None, &text);
                        }
                    }
                    _ => {}
                }
            }
        }
        if matches!(
            outcome,
            Outcome::Fire { .. } | Outcome::Closed | Outcome::Refused(_)
        ) {
            self.retire_menu(window, cx);
        }
        cx.notify();
        true
    }
}
