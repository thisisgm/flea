//! Keyboard and wire routing for the live filter owner.
use super::*;

/// The unmodified Enter a waited-on answer or search rows replay once they land.
pub(crate) fn plain_enter() -> KeyDownEvent {
    KeyDownEvent {
        keystroke: gpui::Keystroke {
            key: "enter".into(),
            ..Default::default()
        },
        is_held: false,
        prefer_character_input: false,
    }
}

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
        // Enter on the line commits and acts on the highlighted row in the same press; an answer still out acts when it lands.
        let enter = matches!(key, LineKey::Enter) && owner.entry.borrow().marked.is_none();
        let waiting = enter
            .then(|| owner.asked().map(|asked| asked.query.clone()))
            .flatten();
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
        if let Some(query) = waiting {
            if let Some(host) = self.panes[side].as_mut() {
                host.filter.enter_after = Some(query);
            }
        } else if enter {
            self.filter_enter_row(event, window, cx);
        }
        consumed
    }

    /// The answer an Enter waited on landed: the row it highlights acts as if Enter came now.
    pub(crate) fn filter_answered(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[side].as_mut() else {
            return;
        };
        if host.filter.owner.asked().is_some() {
            return;
        }
        let Some(query) = host.filter.enter_after.take() else {
            return;
        };
        if side == self.focus_side
            && host.filter.owner.query() == query
            && self.enter_replay_open(window, cx)
        {
            self.filter_enter_row(&plain_enter(), window, cx);
        }
    }

    /// A waiting Enter acts only while the listing still holds the keyboard; a menu, card or other owner since then drops it.
    pub(crate) fn enter_replay_open(&self, window: &Window, cx: &Context<Self>) -> bool {
        !self.input_blocked(cx)
            && self.focus.is_focused(window)
            && self.keyboard.context == flea::keys::Context::Listing
    }

    // The committed line's Enter, run again through the listing so a file reveals and a folder enters as they always do.
    fn filter_enter_row(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes[self.focus_side].as_ref() else {
            return;
        };
        let owner = &host.filter.owner;
        if !owner.up()
            || owner.typing()
            || owner.drawn(host.pane.held.listing).is_none()
            || !flea::ui_logic_sr2::cursor_shown(
                host.filter.shown.as_deref(),
                Some(host.pane.cursor),
            )
            || host.pane.held.row(host.pane.cursor).is_none()
        {
            return;
        }
        self.key(event, window, cx);
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
