//! The rename family: F2, the mac Return and r open the inline editor on the cursor row, or the bulk card over several rows.
use super::Order;
use crate::state::Shell;
use gpui::{Context, Window};

pub(super) fn run(
    shell: &mut Shell,
    order: &Order,
    window: &mut Window,
    cx: &mut Context<Shell>,
) -> bool {
    // PaneMenuActions.qml: the menu's Rename is the listing's own rename, on the rows open_menu left selected, in any view.
    if let Order::Menu(chosen) = order {
        if chosen.action != "rename"
            || !chosen.captured.ready
            || chosen.captured.background
            || !chosen.claim()
        {
            return false;
        }
        let side = chosen.captured.side;
        if chosen.captured.target.is_some() {
            shell.keyboard.context = flea::keys::Context::Listing;
            shell.rename_menu_target(&chosen.captured, window, cx);
            return true;
        }
        shell.adopt_menu_target(&chosen.captured);
        if chosen.captured.rows.len() == 1 {
            shell.keyboard.context = flea::keys::Context::Listing;
            shell.rename_open(side, chosen.captured.id, window, cx);
            shell.rename_sync_gate(window, cx);
            let id = chosen.captured.id;
            // An editor that did not open on this snapshot cannot close it later.
            if !shell
                .rename
                .edit
                .as_ref()
                .is_some_and(|edit| edit.menu == id)
            {
                let close = format!("{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{id}}}");
                shell.menu_send(side, close, cx);
            }
            return true;
        }
        let close = format!(
            "{{\"c\":\"menuaction\",\"op\":\"close\",\"id\":{}}}",
            chosen.captured.id
        );
        shell.menu_send(side, close, cx);
        shell.keyboard.context = flea::keys::Context::Listing;
        shell.focus_side = side;
        let event = crate::window::input::ActionEvent::bare("rename", flea::keys::Context::Listing);
        shell.dispatch(Order::Action(event), window, cx);
        return true;
    }
    let Order::Action(event) = order else {
        return false;
    };
    if event.context != flea::keys::Context::Listing || event.action != "rename" {
        return false;
    }
    let side = shell.focus_side;
    let ran = shell.rename_open(side, 0, window, cx);
    shell.rename_sync_gate(window, cx);
    ran
}
