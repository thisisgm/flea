//! Keyboard and wire routing for the live filter owner.
use super::*;

impl Shell {
    /// The query line's keys while it has the caret, and Escape while a filter stands, ahead of search and marks.
    pub(crate) fn filter_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let side = self.focus_side;
        let Some(host) = &self.panes[side] else {
            return false;
        };
        let owner = &host.filter.owner;
        if !owner.up() || host.search.owner.active() {
            return false;
        }
        if !owner.typing() {
            let plain = crate::window::input::modifiers(event.keystroke.modifiers) == 0;
            if plain
                && event.keystroke.key == "enter"
                && self.focus.is_focused(window)
                && self.keyboard.context == flea::keys::Context::Listing
            {
                return self.filter_reveal_enter(window, cx);
            }
            if !(plain && event.keystroke.key == "escape") {
                return false;
            }
            // Focus.js:187: Escape closes the filter and keeps the marks; the list's next Escape clears them.
            self.filter_update(
                side,
                |filter, pane| {
                    filter.escape(pane);
                    None
                },
                cx,
            );
            return true;
        }
        if self.filter_entry_key(event, cx) {
            return true;
        }
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        let owner = &host.filter.owner;
        let preset = self
            .launch
            .settings
            .data
            .get("keys")
            .and_then(flea::jsondoc::Json::as_str)
            .unwrap_or("default")
            .to_owned();
        let key = match keys::classify(&preset, event, window.modifiers()) {
            Classified::Paste => {
                let text = cx
                    .read_from_clipboard()
                    .and_then(|item| item.text())
                    .unwrap_or_default();
                self.filter_update(side, |filter, pane| filter.typed(&text, pane), cx);
                return true;
            }
            Classified::Key(key) => key,
        };
        // Enter on the line activates a single exact match; files reveal and folders enter.
        let single = matches!(key, LineKey::Enter)
            && owner.entry.borrow().marked.is_none()
            && owner.asked().is_none()
            && owner
                .drawn(host.pane.held.listing)
                .is_some_and(|drawn| drawn.narrowed.exact.len() == 1);
        let mut consumed = true;
        self.filter_update(
            side,
            |filter, pane| {
                let (took, asked) = filter.line_key(key, pane);
                consumed = took;
                asked
            },
            cx,
        );
        if single && !self.filter_reveal_enter(window, cx) {
            self.input_action("open", window, cx);
        }
        consumed
    }

    fn filter_reveal_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let side = self.focus_side;
        let Some(host) = self.panes[side].as_ref() else {
            return false;
        };
        if host.pane.busy() || host.stopped || host.filter.owner.asked().is_some() {
            return true;
        }
        if !flea::ui_logic_sr2::cursor_shown(host.filter.shown.as_deref(), Some(host.pane.cursor))
            || !host
                .pane
                .held
                .row(host.pane.cursor)
                .is_some_and(|row| !flea::ui_logic_pk1::directory(row.dir, row.mode, &row.icon))
        {
            return false;
        }
        self.filter_update(
            side,
            |filter, pane| {
                filter.close(pane);
                None
            },
            cx,
        );
        self.recent_reveal(side, window, cx);
        true
    }

    /// A `filtered` line for this pane: the answer in force lands whole, a stale one is dropped, a broken one is a protocol error.
    pub(crate) fn filter_line(&mut self, side: usize, line: &str, cx: &mut Context<Self>) -> bool {
        if !line.contains("\"filtered\"") {
            return false;
        }
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return false;
        };
        match wire::read(line, host.filter.owner.oldest()) {
            Ok(Line::Other) => false,
            Ok(Line::Stale) => true,
            Ok(Line::Answer(answer)) => {
                self.filter_update(
                    side,
                    |filter, pane| {
                        let _ = filter.receive(answer, pane);
                        None
                    },
                    cx,
                );
                true
            }
            Err(detail) => {
                host.record_error(
                    crate::backend::Failure {
                        key: "window.backend_protocol",
                        detail,
                    },
                    &self.locale,
                );
                true
            }
        }
    }
}
