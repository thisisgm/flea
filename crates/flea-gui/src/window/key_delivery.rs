//! flea.keyDeliveryState: v0.3.12's key, clipboard, search, focus, preview and history state, read from each live owner.
use crate::{ipc::Adapter, state::Shell};
use flea::{
    ipc::wire::Reply,
    json::{escape, string_array},
    ui_logic_sr1::Mode,
};
use gpui::{App, Window};

pub(crate) struct Reader<'a> {
    pub shell: &'a Shell,
    /// Whether the Columns view hosts its preview column, as previewState reads it.
    pub preview_active: bool,
    pub window: &'a Window,
    pub cx: &'a App,
}

impl Adapter for Reader<'_> {
    fn call(&self, function: &str, _: &[String]) -> Option<Reply> {
        (function == "keyDeliveryState").then(|| Reply::value(self.state()))
    }
}

impl Reader<'_> {
    // Ipc.qml keyDeliveryState: the same keys, JSON types and order, on one line as JSON.stringify printed it.
    fn state(&self) -> String {
        let (shell, window, cx) = (self.shell, self.window, self.cx);
        let Some(host) = shell.panes[shell.focus_side].as_ref() else {
            return "{}".into();
        };
        let (sequence, identity) =
            shell
                .keyboard
                .pending()
                .map_or_else(Default::default, |(action, seat)| {
                    let identity = format!(
                        "[{},{},{},{},{}]",
                        seat.listing, seat.cursor, seat.selection, seat.view, seat.focus
                    );
                    (action, identity)
                });
        let clip = shell.clipboard.board.current();
        let search = &host.search.owner;
        let session = &search.session;
        let mode = match session.mode {
            Mode::Off => "",
            Mode::Typing => "typing",
            Mode::Results => "results",
        };
        let filter = &host.filter.owner;
        let preview = host.preview.as_ref().map(|view| view.read(cx));
        let preview_focused = preview.is_some_and(|view| view.focus.contains_focused(window, cx));
        let preview_index = preview
            .filter(|view| self.preview_active && view.visible)
            .and_then(|view| view.drawn.as_ref())
            .map_or(-1, |frame| frame.subject.index as i64);
        // A modal card takes the keyboard, so no stop inside the window holds focus while it is up.
        let blocked = shell.input_blocked(cx);
        let stop = shell.focus_view(window, cx);
        let line = if search.typing() {
            "search"
        } else if filter.typing() {
            "filter"
        } else {
            ""
        };
        let focus_item = match stop {
            _ if blocked => "",
            "list" if !line.is_empty() => line,
            "" if preview_focused => "preview",
            stop => stop,
        };
        // The rail sits inside Pane.qml; the chrome and the tab strip belong to the window.
        let pane_focus = !matches!(focus_item, "" | "chrome" | "strip");
        let list_focus = pane_focus && focus_item == "list";
        let paths = |entries: &[crate::navigation::history::Entry]| {
            string_array(
                &entries
                    .iter()
                    .map(|entry| entry.path.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let history = &host.nav.history;
        format!(
            concat!(
                "{{\"keySequence\":\"{}\",\"keySequenceIdentity\":\"{}\",\"trashArmedAt\":{},",
                "\"clipboard\":{{\"paths\":{},\"cut\":{}}},\"searchMode\":\"{}\",\"filterTyping\":{},",
                "\"filterQuery\":\"{}\",\"searchRunning\":{},\"searchQuery\":\"{}\",\"searchScanned\":{},",
                "\"searchCancelled\":{},\"paneFocus\":{},\"listFocus\":{},\"activeFocusItem\":\"{}\",",
                "\"preview\":{{\"index\":{},\"focused\":{}}},\"history\":{{\"back\":{},\"forward\":{}}}}}"
            ),
            sequence,
            escape(&identity),
            shell.operations.fileops.armed_at(),
            string_array(&clip.paths),
            clip.moving,
            mode,
            filter.typing(),
            escape(filter.query()),
            session.running,
            escape(&session.query),
            session.scanned,
            session.cancelled,
            pane_focus,
            list_focus,
            focus_item,
            preview_index,
            preview_focused,
            paths(&history.back),
            paths(&history.forward),
        )
    }
}

#[cfg(test)]
#[path = "key_delivery_tests.rs"]
mod tests;
