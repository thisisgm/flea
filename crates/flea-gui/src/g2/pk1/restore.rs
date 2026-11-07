//! Refused submissions return focus only while Cancel still owns the focus we temporarily moved.
use super::Picker;
use gpui::{Context, Window};

impl Picker {
    pub(super) fn record_submission_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.chrome.was_submitting {
            self.chrome.restore = window.focused(cx);
        }
    }

    pub(super) fn submission_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A create out with its card closed holds the chooser on Cancel, as it did before the card; an open card keeps its own focus.
        let busy = self.accept_state().submitting()
            || !self.browse.route.asked.is_empty()
            || (self.chrome.folder_request.is_some() && self.chrome.folder.is_none());
        if busy && !self.chrome.was_submitting {
            if self.chrome.restore.is_none() {
                self.chrome.restore = window.focused(cx);
            }
            window.focus(&self.chrome.cancel, cx);
        } else if busy && self.chrome.was_submitting && !self.chrome.cancel.is_focused(window) {
            self.chrome.restore = None;
            self.chrome.restore_path = None;
        } else if !busy && self.chrome.was_submitting {
            if !self.answered()
                && !self.backend_unavailable()
                && self.chrome.cancel.is_focused(window)
            {
                if let Some(focus) = self.chrome.restore.take() {
                    if let Some(text) = self.chrome.restore_path.take() {
                        if self.chrome.reopen_path {
                            let path = self.browse.path_view(cx);
                            path.update(cx, |view, cx| {
                                view.open(window, cx);
                                view.entry.begin(&text);
                                cx.notify();
                            });
                        } else {
                            window.focus(&self.focus, cx);
                        }
                    } else {
                        window.focus(&focus, cx);
                    }
                }
            }
            self.chrome.restore = None;
            self.chrome.restore_path = None;
            self.chrome.reopen_path = false;
        }
        self.chrome.was_submitting = busy;
    }
}
