//! Main-window protocol and focus-stop readers use the owners that route its keyboard.
use super::Adapter;
use crate::state::Shell;
use flea::ipc::wire::Reply;
use gpui::{App, Window};

pub(crate) struct BrowseState<'a> {
    pub shell: &'a Shell,
    pub window: &'a Window,
    pub cx: &'a App,
}

impl Adapter for BrowseState<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        match function {
            "mode" => Some(Reply::value("browse")),
            "focusStop" => {
                let stop = match self.shell.focus_view(self.window, self.cx) {
                    "chrome" => {
                        let glyph = self.shell.chrome.view.as_ref().and_then(|chrome| {
                            let frame = chrome.read(self.cx).frame.borrow();
                            frame
                                .controls
                                .iter()
                                .position(|control| control.focused)
                                .map(|at| crate::path::chrome::GLYPHS[at])
                        });
                        format!("chrome:{}", glyph.unwrap_or_default())
                    }
                    "strip" => format!("strip:{}", self.shell.strip.bar.focused_index),
                    // No pane stop: FocusAdapter answers from the preview, Trash and Settings owners.
                    "" => return None,
                    view => view.into(),
                };
                Some(Reply::value(stop))
            }
            // No Recent listing is mounted, so both readers have their specified off-state value.
            "recentMode" | "recentFrom" => Some(Reply::value("")),
            _ => None,
        }
    }
}
