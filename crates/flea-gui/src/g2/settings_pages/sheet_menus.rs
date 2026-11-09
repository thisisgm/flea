//! The keyboard sheet's menu source: the cursor row's menu, hidden rows included, with each dead row's reason.
use crate::state::Shell;
use flea::ui_logic_context_menus::{self as mn1, Entry};
use flea::ui_logic_settings_pages::{
    sheet::{MenuEntry, MenuLeaf},
    Text,
};

// Board keymap-dead: a paste row with nothing on the clipboard says so at its end.
const NOTHING_TO_PASTE: &str = "paste.nothing";

impl Shell {
    /// The focused pane's cursor-row menu as the sheet searches it, so a row Settings hides is still found.
    pub(crate) fn sheet_menus(&self) -> Vec<MenuEntry> {
        let side = self.focus_side;
        let Some(host) = &self.panes[side] else {
            return Vec::new();
        };
        let cursor = host.pane.cursor;
        // As the menu key decides it: a listing with rows has a cursor row, even one scrolled out of the held window.
        let background = host.pane.held.total == 0;
        let rows = if background {
            Vec::new()
        } else if host.selected.is_empty() {
            vec![cursor]
        } else {
            host.selected.clone()
        };
        let mut context = self.listing_menu_context(side, &rows, cursor, background);
        self.archive_context(side, &mut context);
        let clipboard = context.clipboard_available || context.clipboard_watch_failed;
        sheet_entries(
            &self.listing_menu_entries(side, background, context, Vec::new()),
            clipboard,
        )
    }
}

/// Menu rows as the sheet's query reads them; Paste As, which the menu drops while nothing is held, stays as a dead row.
pub(crate) fn sheet_entries(entries: &[Entry], clipboard: bool) -> Vec<MenuEntry> {
    let mut out: Vec<MenuEntry> = entries
        .iter()
        .filter(|entry| !entry.separator)
        .map(|entry| sheet_entry(entry, clipboard))
        .collect();
    if !clipboard && !out.iter().any(|entry| entry.action == "pasteAs") {
        out.push(MenuEntry {
            label: Text::Key("menu.row.paste_as"),
            action: "pasteAs".into(),
            disabled: true,
            reason: Text::Key(NOTHING_TO_PASTE),
            ..MenuEntry::default()
        });
    }
    out
}

fn sheet_entry(entry: &Entry, clipboard: bool) -> MenuEntry {
    let paste = matches!(entry.action.as_ref(), "paste" | "pasteAs");
    let reason = if !entry.disabled {
        Text::Empty
    } else if paste && !clipboard {
        Text::Key(NOTHING_TO_PASTE)
    } else {
        entry.hint.as_ref().map_or(Text::Empty, text)
    };
    MenuEntry {
        label: entry.label.as_ref().map_or(Text::Empty, text),
        action: entry.action.to_string(),
        disabled: entry.disabled,
        reason,
        danger: entry.danger,
        separator: false,
        submenu: entry
            .submenu
            .iter()
            .flatten()
            .filter(|leaf| !leaf.separator)
            .map(|leaf| MenuLeaf {
                id: leaf.id.to_string(),
                label: leaf.label.as_ref().map_or(Text::Empty, text),
                disabled: leaf.disabled,
                reason: if leaf.disabled {
                    leaf.hint.as_ref().map_or(Text::Empty, text)
                } else {
                    Text::Empty
                },
                ..MenuLeaf::default()
            })
            .collect(),
    }
}

fn text(value: &mn1::Text) -> Text {
    match value {
        mn1::Text::Key(key) => Text::Key(key),
        mn1::Text::Count(key, count) => Text::Count(key, *count, Vec::new()),
        mn1::Text::KeyArg(key, arg) => Text::Args(key, vec![Text::Key(arg)]),
        mn1::Text::Data(value) => Text::data(value.clone()),
    }
}
