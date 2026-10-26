//! The clipboard keys the verbs do not cover: Copy path, the folder path, and the Copy as and Paste as flyouts.
use super::Order;
use crate::state::Shell;
use flea::{keys::Context as Keys, ui_logic_fb1::status::Hint};
use gpui::{ClipboardItem, Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    match order {
        Order::Action(event) if event.context == Keys::Listing => {
            key(shell, event.action, window, cx)
        }
        _ => false,
    }
}

// Format.js shellQuoted: a path of plain characters stays bare, any other is single quoted.
fn shell_quoted(path: &str) -> String {
    let plain = !path.is_empty()
        && path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&b));
    if plain {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

// Focus.js:212-240: the listing keys; copy, cut and paste run through the Clipboard's own key hook before dispatch.
fn key(shell: &mut Shell, action: &str, window: &mut Window, cx: &mut Context<Shell>) -> bool {
    let side = shell.focus_side;
    match action {
        "copyPath" => shell.clipboard_copy_path(window, cx),
        // Pane.qml copyDirPath: the folder the pane shows, shell quoted, as text.
        "copydirpath" => {
            if let Some(host) = &shell.panes[side] {
                cx.write_to_clipboard(ClipboardItem::new_string(shell_quoted(host.pane.path())));
            }
        }
        // Pane.qml openCopyAs: the cursor row's menu opens on its Copy as flyout.
        "copyAs" => {
            let row = shell.panes[side]
                .as_ref()
                .is_some_and(|host| host.pane.held.row(host.pane.cursor).is_some());
            if row {
                shell.open_flyout("copyAs", window, cx);
            } else {
                say(shell, side, "clipboard.no_cursor_row", cx);
            }
        }
        // Pane.qml openPasteAs: an empty clipboard says so instead of opening a greyed flyout.
        "pasteAs" if !shell.clipboard.board.available() => {
            say(shell, side, "menu.refusal.empty_clipboard", cx);
        }
        "pasteAs" => shell.open_flyout("pasteAs", window, cx),
        _ => return false,
    }
    true
}

fn say(shell: &mut Shell, side: usize, key: &str, cx: &mut Context<Shell>) {
    let sentence = shell.locale.catalog.tr(key);
    if let Some(host) = shell.panes[side].as_mut() {
        host.status = sentence.clone().into();
    }
    shell.status_messages(cx, |messages, now| {
        messages.say(&sentence, false, "", Hint::None, now)
    });
    cx.notify();
}

impl Shell {
    /// c and P from the listing: the cursor menu opens, then its flyout once the menu has its rows.
    pub(crate) fn open_flyout(
        &mut self,
        action: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu_keyboard_open(window, cx);
        if self.menu.captured.is_none() {
            return;
        }
        self.menu.flyout = Some(action);
        if self.menu_opened() {
            self.flyout_pending(window, cx);
        }
    }

    /// The flyout a key asked for before the menu drew; the menu's refresh calls this once it opens.
    pub(crate) fn flyout_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(action) = self.menu.flyout.take() else {
            return;
        };
        let Some(side) = self.menu.captured.as_ref().map(|capture| capture.side) else {
            return;
        };
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let list = host.list.borrow();
        let env = self.menu_env(&list.typeface);
        let available = self.menu.context.clipboard_available;
        let outcome = self
            .menu
            .menu
            .borrow_mut()
            .open_submenu_for(action, available, &env, window);
        drop(list);
        self.menu_outcome(outcome, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::shell_quoted;

    #[test]
    fn shell_quoting_follows_format_js() {
        assert_eq!(shell_quoted("/home/gm/a-b_c.txt"), "/home/gm/a-b_c.txt");
        assert_eq!(shell_quoted("/home/gm/My Files"), "'/home/gm/My Files'");
        assert_eq!(shell_quoted("/x/it's"), "'/x/it'\\''s'");
    }
}
