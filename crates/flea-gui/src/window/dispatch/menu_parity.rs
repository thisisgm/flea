//! Listing keys that the menu rows share: langMarks and the cursor row's symlink target.
use super::{menus, original, Order};
use crate::state::Shell;
use flea::keys::Context as Keys;
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    let Order::Action(event) = order else {
        return false;
    };
    if event.context != Keys::Listing {
        return false;
    }
    match event.action {
        "toggleLangMarks" => menus::toggle_lang_marks(shell, window, cx),
        "showOriginal" => show_original(shell, cx),
        _ => return false,
    }
    true
}

// Pane.qml showOriginal: the cursor must sit on a link, else the listing says so.
fn show_original(shell: &mut Shell, cx: &mut Context<Shell>) {
    let side = shell.focus_side;
    let link = shell.panes[side].as_ref().and_then(|host| {
        let row = host.pane.held.row(host.pane.cursor)?;
        row.link.as_ref()?;
        Some(crate::navigation::history::join(
            host.pane.path(),
            &row.name,
        ))
    });
    match link {
        Some(link) => original::request(shell, side, link, cx),
        None => shell.menu_explain(shell.locale.catalog.tr("menu.original.needs_link"), cx),
    }
}
