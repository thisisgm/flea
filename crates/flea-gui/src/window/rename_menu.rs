//! The window's own text fields use the shared menu card and its normal keyboard and pointer routing: the rename entry, Find and the filter.
use crate::{
    g2::{
        context_menus::menu::{Face, Outcome},
        file_search::input::FindRoute,
        listing_filter::input::FilterRoute,
    },
    path::{
        entry::Entry,
        text_menu::{apply, rows, Applied},
    },
    state::Shell,
    strips::query_input::Route,
};
use flea::{keys, ui_logic_context_menus::fit::Rect};
use gpui::{Bounds, Context, MouseDownEvent, Pixels, Point, Window};

/// Which of the window's lines the open text menu acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Line {
    Rename,
    Find,
    Filter,
}

impl Shell {
    // The line whose editing session the menu was opened on, while it still edits.
    fn text_menu_line(&self) -> Option<(usize, Line)> {
        let (side, session) = self.menu.text?;
        if self.rename.edit.as_ref().is_some_and(|edit| {
            edit.side == side && edit.entry.session() == Some(session) && edit.pending.is_none()
        }) {
            return Some((side, Line::Rename));
        }
        let host = self.panes.get(side)?.as_ref()?;
        if host.search.owner.typing() && host.search.owner.entry.borrow().session() == Some(session)
        {
            return Some((side, Line::Find));
        }
        (host.filter.owner.typing()
            && !host.search.owner.active()
            && host.filter.owner.entry.borrow().session() == Some(session))
        .then_some((side, Line::Filter))
    }

    /// Whether `at` is on a line that types now: the rename field, Find or the filter.
    pub(crate) fn text_line_at(&self, at: Point<Pixels>) -> bool {
        let inside = |bounds: Option<Bounds<Pixels>>| bounds.is_some_and(|b| b.contains(&at));
        let rename = self
            .rename
            .edit
            .as_ref()
            .is_some_and(|edit| inside(edit.line.frame));
        rename
            || self.panes.iter().flatten().any(|host| {
                let find = host.search.owner.typing()
                    && inside(host.search.strip.borrow().edit.bounds.get());
                let filter = host.filter.owner.typing()
                    && !host.search.owner.active()
                    && inside(host.filter.strip.borrow().query.room());
                find || filter
            })
    }

    fn with_text_menu_line<R>(
        &mut self,
        side: usize,
        line: Line,
        change: impl FnOnce(&mut Entry) -> R,
    ) -> Option<R> {
        match line {
            Line::Rename => self
                .rename
                .edit
                .as_mut()
                .map(|edit| change(&mut edit.entry)),
            Line::Find => self.panes[side]
                .as_ref()
                .map(|host| change(&mut host.search.owner.entry.borrow_mut())),
            Line::Filter => self.panes[side]
                .as_ref()
                .map(|host| change(&mut host.filter.owner.entry.borrow_mut())),
        }
    }

    pub(crate) fn rename_menu_entries(&self) -> Vec<flea::ui_logic_context_menus::Entry> {
        let Some((side, line)) = self.text_menu_line() else {
            return Vec::new();
        };
        let paste = self.menu.text_paste;
        match (line, self.panes[side].as_ref()) {
            (Line::Rename, _) => self
                .rename
                .edit
                .as_ref()
                .map(|edit| rows(&edit.entry, paste))
                .unwrap_or_default(),
            (Line::Find, Some(host)) => rows(&host.search.owner.entry.borrow(), paste),
            (Line::Filter, Some(host)) => rows(&host.filter.owner.entry.borrow(), paste),
            _ => Vec::new(),
        }
    }

    /// Opens the text menu with the selection frozen at the point the menu was requested.
    pub(crate) fn rename_menu_open(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.menu_opened() {
            return;
        }
        let Some(edit) = self
            .rename
            .edit
            .as_mut()
            .filter(|edit| edit.pending.is_none())
        else {
            return;
        };
        let Some(session) = edit.entry.session() else {
            return;
        };
        let side = edit.side;
        edit.line.end_selection_drag();
        self.text_menu_open(side, session, event, window, cx);
    }

    /// A right press on Find's or the filter's line while it types: the rename field's menu, over that line; true when it opened.
    pub(crate) fn query_menu_open(
        &mut self,
        side: usize,
        session: u64,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu_opened() || self.input_blocked(cx) {
            return false;
        }
        self.focus_side = side;
        window.focus(&self.focus, cx);
        self.text_menu_open(side, session, event, window, cx);
        self.menu.text.is_some()
    }

    fn text_menu_open(
        &mut self,
        side: usize,
        session: u64,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.retire_menu(window, cx);
        self.observe_menu(window, cx);
        self.menu.text = Some((side, session));
        // A line still composing keeps its input method's keys.
        let composing = self.text_menu_line().is_some_and(|(side, line)| {
            self.with_text_menu_line(side, line, |entry| entry.marked.is_some())
                .unwrap_or(true)
        });
        if self.text_menu_line().is_none() || composing {
            self.menu.text = None;
            return;
        }
        self.menu.text_paste = crate::path::text_menu::can_paste(cx);
        let entries = self.rename_menu_entries();
        let typeface = self.panes[side]
            .as_ref()
            .unwrap()
            .list
            .borrow()
            .typeface
            .clone();
        let env = self.menu_env(&typeface);
        let size = window.viewport_size();
        let at = (event.position.x.into(), event.position.y.into());
        self.menu.menu.borrow_mut().open(
            Face::Text,
            entries,
            at,
            Rect {
                x: 0.,
                y: 0.,
                w: size.width.into(),
                h: size.height.into(),
            },
            "",
            &env,
            window,
        );
        self.menu.pointer = Some(at);
        self.menu.resting = Some(at);
        self.menu.pointer_armed = false;
        self.keyboard.context = keys::Context::Menu;
        self.keyboard.disarm();
        cx.notify();
    }

    pub(crate) fn rename_menu_outcome(
        &mut self,
        outcome: &Outcome,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.menu.text.is_none() {
            return false;
        }
        let line = self.text_menu_line();
        if let (Outcome::Fire { action, .. }, Some((side, line))) = (outcome, line) {
            let applied = self
                .with_text_menu_line(side, line, |entry| apply(entry, action, cx))
                .unwrap_or(Applied::Nothing);
            self.query_line_changed(side, line, applied, window, cx);
        }
        if matches!(
            outcome,
            Outcome::Fire { .. } | Outcome::Closed | Outcome::Refused(_)
        ) {
            self.retire_menu(window, cx);
        }
        cx.notify();
        true
    }

    // Find and the filter follow an edited line as they follow committed typed text; a moved selection only redraws.
    fn query_line_changed(
        &mut self,
        side: usize,
        line: Line,
        applied: Applied,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(generation) = self.with_text_menu_line(side, line, |entry| entry.generation)
        else {
            return;
        };
        let edited = applied == Applied::Edited;
        match (line, applied) {
            (Line::Rename, _) | (_, Applied::Nothing) => {}
            (Line::Find, _) => {
                if edited {
                    FindRoute::commit(self, side, generation, window, cx);
                }
                FindRoute::redraw(self, side, cx);
            }
            (Line::Filter, _) => {
                if edited {
                    FilterRoute::commit(self, side, generation, window, cx);
                }
                FilterRoute::redraw(self, side, cx);
            }
        }
    }
}

#[cfg(test)]
#[path = "text_menu_tests.rs"]
pub(crate) mod text_menu_tests;
