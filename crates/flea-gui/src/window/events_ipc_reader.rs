//! Listing readiness starts the current window IPC reader.
use super::*;

impl Shell {
    pub(super) fn ipc_chrome_glyph(&self, cx: &gpui::App) -> Option<&'static str> {
        self.chrome.view.as_ref().and_then(|chrome| {
            let frame = chrome.read(cx).frame.borrow();
            frame
                .controls
                .iter()
                .position(|control| control.focused)
                .map(|at| crate::path::chrome::GLYPHS[at])
        })
    }

    /// The first listing answered or gave up waiting, so calls from here on reach the window's readers.
    pub(crate) fn ensure_ipc_reader(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.ipc_cold = false;
        self.start_ipc_reader(window, cx);
    }

    pub(crate) fn start_ipc_reader(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(events) = self.ipc_events.take() else {
            return;
        };
        self._ipc_reader = Some(cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = events.recv().await {
                if cx
                    .update(|window, cx| this.update(cx, |shell, cx| shell.ipc(event, window, cx)))
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
}
