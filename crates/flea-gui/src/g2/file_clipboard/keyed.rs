//! The listing keys the Clipboard answers beside its verbs: Copy path over the pane's targets, and the image a paste of an empty clipboard sends.
use super::join::{picks, PaneWire};
use super::seams::Wire;
use crate::state::Shell;
use flea::ui_logic_file_clipboard::{self as core, PasteAsk};
use gpui::{Context, Window};

impl Shell {
    /// Focus.js copyPath: the Copy As path form over the selection, else the cursor row, named by the backend as the menu leaf names them.
    pub(crate) fn clipboard_copy_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return;
        };
        let rows = picks(host);
        self.clipboard_run(
            side,
            Box::new(
                move |board, i18n, backend, take, landing, wire, sink, owner| {
                    let said = board.menu_action(
                        i18n,
                        backend,
                        "copyPath",
                        &[],
                        "",
                        rows,
                        take,
                        landing,
                        wire,
                        sink,
                        owner,
                    );
                    ((), said)
                },
            ),
            window,
            cx,
        );
    }

    /// Clipboard.js paste: with no paths the backend pastes the clipboard image as a stamped file, outside the collision ask.
    pub(super) fn clipboard_image(&mut self, side: usize, ask: &PasteAsk, cx: &mut Context<Self>) {
        let PasteAsk::Image { dest, name } = ask else {
            return;
        };
        let backend = self.panes[side]
            .as_ref()
            .and_then(|host| host.backend.as_ref());
        if !PaneWire(backend).send(core::paste_image_line(dest, name)) {
            self.say_stopped(cx);
        }
    }
}
