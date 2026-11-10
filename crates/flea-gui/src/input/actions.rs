use crate::{
    list::pane::{page_items, Move},
    state::Shell,
};
use gpui::{Context, KeyDownEvent, Window};

// Marks.js follow only(cursor, keep): a lone mark moves with a plain move, and a carried landing stays the landing, so Escape still climbs through it.
pub(crate) fn follow_cursor(host: &mut crate::window::pane::PaneHost, from: usize) {
    host.input
        .selection
        .follow(&mut host.selected, host.pane.cursor);
    if let Some((listing, row)) = host.nav.landed {
        if row == from && host.selected == [host.pane.cursor] {
            host.nav.landed = Some((listing, host.pane.cursor));
        }
    }
}

impl Shell {
    /// Focus.js handleKey: a tracking selection band takes every key; Escape rolls it back and the rest do nothing.
    pub(crate) fn band_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let tracking = self.panes[self.focus_side]
            .as_ref()
            .is_some_and(|host| host.input.band.is_some());
        if !tracking {
            return false;
        }
        if event.keystroke.key == "escape" {
            self.input_action("escape", window, cx);
        }
        cx.stop_propagation();
        true
    }

    /// One Tab or Shift+Tab along the window's ring from `current`, the stop the keyboard is on.
    pub(crate) fn ring_step(
        &mut self,
        current: &str,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dual = self.view == crate::window::startup::View::Dual;
        if dual
            && current == "list"
            && ((forward && self.focus_side == 0) || (!forward && self.focus_side == 1))
        {
            self.focus_side = 1 - self.focus_side;
            self.enter_list_stop(forward, window, cx);
        } else if let Some(stop) = self.input.step(current, forward) {
            window.focus(&stop.handle, cx);
            self.keyboard.context = stop.context;
            if stop.name == "rail" {
                self.focus_side = 0;
            } else if stop.name == "chrome" {
                self.enter_chrome(forward, window, cx);
            } else if stop.name == "strip" {
                self.enter_strip(forward);
            }
        } else {
            self.focus_side = usize::from(dual && !forward);
            self.enter_list_stop(forward, window, cx);
            self.keyboard.context = flea::keys::Context::Listing;
        }
        self.keyboard.disarm();
        cx.notify();
    }

    pub(crate) fn input_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.wheel_blocked(cx) {
            return true;
        }
        if let Some(host) = &mut self.panes[self.focus_side] {
            if host.input.band.is_some() {
                if action == "escape" {
                    host.input.cancel(&mut host.selected, &mut host.pane.cursor);
                    cx.notify();
                }
                return true;
            }
        }
        if action == "focusNext" || action == "focusPrevious" {
            let current = self.focus_view(window, cx);
            self.ring_step(current, action == "focusNext", window, cx);
            return true;
        }
        if action == "escape" && !matches!(self.focus_view(window, cx), "" | "list") {
            window.focus(&self.overlay_return_focus(cx), cx);
            self.keyboard.context = flea::keys::Context::Listing;
            cx.notify();
            return true;
        }
        if self.keyboard.context != flea::keys::Context::Listing {
            return false;
        }
        let climbs = action == "escape" && self.escape_climbs();
        if let Some(step) = step_of(action) {
            return self.move_on(self.focus_side, action, step, window, cx);
        }
        let Some(host) = &mut self.panes[self.focus_side] else {
            return false;
        };
        // Empty Escape still cancels a ground click, then falls through to the listing's notice or climb.
        if action == "escape" && host.selected.is_empty() {
            host.input.selection.reset();
        }
        match action {
            "toggleSelect" if host.pane.cursor < host.pane.held.total => {
                if !host
                    .input
                    .selection
                    .promote(&host.selected, host.pane.cursor)
                {
                    host.input
                        .selection
                        .toggle(&mut host.selected, host.pane.cursor);
                }
            }
            "selectAll" => host.input.selection.all(
                &mut host.selected,
                host.pane.held.total,
                host.input.shown.as_deref(),
                false,
            ),
            "invertSelection" => host.input.selection.all(
                &mut host.selected,
                host.pane.held.total,
                host.input.shown.as_deref(),
                true,
            ),
            // Focus.js:187-193 unwinds filter, search, Recent, status and escape-up before escapePressed clears marks last; the climb is the listing family's (window/dispatch/listing.rs).
            "escape" if !host.selected.is_empty() && !climbs => {
                host.input.selection.clear(&mut host.selected)
            }
            _ => return false,
        }
        host.nav.landed = None;
        host.input.pulled = None;
        host.input.keyboard();
        self.keyboard.disarm();
        cx.notify();
        true
    }
}

fn step_of(action: &str) -> Option<Move> {
    match action {
        "cursorUp" | "extendUp" => Some(Move::Up),
        "cursorDown" | "extendDown" => Some(Move::Down),
        "pageUp" => Some(Move::PageUp),
        "pageDown" => Some(Move::PageDown),
        "cursorFirst" => Some(Move::Home),
        "cursorLast" => Some(Move::End),
        _ => None,
    }
}

/// The cursor keys the re-read guard held for their own folder, in order, each run of one key counted once.
pub(crate) struct Queued {
    runs: Vec<(&'static str, usize)>,
    path: String,
}

// The held action's own name, so a run keeps no copy of the key.
fn held(action: &str) -> Option<&'static str> {
    Some(match action {
        "cursorUp" => "cursorUp",
        "cursorDown" => "cursorDown",
        "extendUp" => "extendUp",
        "extendDown" => "extendDown",
        "pageUp" => "pageUp",
        "pageDown" => "pageDown",
        "cursorFirst" => "cursorFirst",
        "cursorLast" => "cursorLast",
        _ => return None,
    })
}

impl Shell {
    fn move_on(
        &mut self,
        side: usize,
        action: &str,
        step: Move,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let following = self.quick_look_open(cx);
        let Some(host) = &mut self.panes[side] else {
            return false;
        };
        // The re-read renumbers the rows, so the key waits for it instead of moving the rows it replaces.
        if host.pane.busy() {
            // Only a same-folder re-read holds the key; a load into another folder swallows it.
            if host.pane.place() == host.pane.path() {
                host.input.queue(action, host.pane.place());
            }
            return true;
        }
        let from = host.pane.cursor;
        let before_scroll = host.pane.scroll_y;
        let wrap = flea::ui_logic_settings_pages::pages::is_true(
            &self.launch.settings.data,
            &["wrapAtEnds"],
        );
        let frame = if self.view == crate::window::startup::View::Columns {
            host.columns_key_frame(self.grows[side])
        } else {
            host.input.frame
        };
        if let Some(frame) = frame {
            if frame.total == 0 {
                return true;
            }
            let at = host.input.view_index(from);
            // Quick Look follows adjacent files, regardless of the grid behind it.
            let columns = if following { 1 } else { frame.columns.max(1) };
            let page = page_items((frame.viewport() / frame.row).ceil() as usize) * columns;
            let to = match step {
                Move::Up => at.saturating_sub(columns),
                Move::Down => at.saturating_add(columns),
                Move::PageUp => at.saturating_sub(page),
                Move::PageDown => at.saturating_add(page),
                Move::Home => 0,
                Move::End => frame.total - 1,
            }
            .min(frame.total - 1);
            let to = match (wrap, action, at, frame.total) {
                (true, "cursorDown", at, total) if at + 1 == total => 0,
                (true, "cursorUp", 0, total) => total - 1,
                _ => to,
            };
            host.pane.cursor = host.input.index(to);
        } else {
            if !host.pane.move_cursor(step) {
                return true;
            }
            let total = host.pane.held.total;
            host.pane.cursor = match (wrap, action, from) {
                (true, "cursorDown", at) if at + 1 == total => 0,
                (true, "cursorUp", 0) => total - 1,
                _ => host.pane.cursor,
            };
            host.pane.reveal_cursor();
        }
        if let Some(mut frame) = frame {
            frame.scroll = host.pane.scroll_y;
            host.pane.scroll_to(super::scroll::reveal(
                frame,
                host.input.view_index(host.pane.cursor),
                3,
            ));
        }
        let extend = matches!(action, "extendUp" | "extendDown");
        let pulled = host.input.take_pull(from, host.pane.held.listing);
        if extend {
            // Selection.js dropLone: a deliberate change spends the landing, so Escape no longer climbs past it.
            host.nav.landed = None;
            host.input.selection.extend(
                &mut host.selected,
                from,
                host.pane.cursor,
                pulled,
                host.input.shown.as_deref(),
            );
        } else {
            follow_cursor(host, from);
        }
        host.input.keyboard();
        if host.pane.scroll_y != before_scroll {
            host.input.moved(cx.background_executor().now());
        }
        self.seam.selected(host.pane.cursor);
        self.arm_input_tick(side, window, cx);
        cx.notify();
        true
    }

    /// The row-action guard swallows keys while the listing is out; a cursor key is held for the re-read instead.
    pub(crate) fn hold_cursor_key(&mut self, action: &str) {
        if let Some(host) = &mut self.panes[self.focus_side] {
            if host.pane.busy()
                && host.pane.place() == host.pane.path()
                && host.input.band.is_none()
            {
                host.input.queue(action, host.pane.place());
            }
        }
    }

    /// After each reply: a held key lands once its folder's re-read and anchor have, and goes stale when the folder or focus changes.
    pub(crate) fn replay_queued(
        &mut self,
        side: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(host) = self.panes.get_mut(side).and_then(Option::as_mut) else {
            return;
        };
        let Some(queued) = &host.input.queued else {
            return;
        };
        if queued.path != host.pane.place() {
            host.input.queued = None;
            return;
        }
        let anchored = host
            .nav
            .anchor
            .as_ref()
            .is_some_and(|anchor| anchor.path == host.pane.path());
        if host.pane.busy() || anchored {
            return;
        }
        let Some(queued) = host.input.queued.take() else {
            return;
        };
        if side != self.focus_side
            || self.keyboard.context != flea::keys::Context::Listing
            || host.input.band.is_some()
        {
            return;
        }
        // The drawn frame still counts the replaced rows until the next prepaint.
        if let Some(frame) = &mut host.input.frame {
            frame.total = host.pane.held.total;
        }
        if self.input_blocked(cx) {
            return;
        }
        for (action, count) in queued.runs {
            let Some(step) = step_of(action) else {
                continue;
            };
            for _ in 0..count {
                self.move_on(side, action, step, window, cx);
            }
        }
    }
}

impl crate::input::PaneInput {
    fn queue(&mut self, action: &str, place: &str) {
        let Some(action) = held(action) else {
            return;
        };
        let queued = match &mut self.queued {
            Some(queued) if queued.path == place => queued,
            slot => slot.insert(Queued {
                runs: Vec::new(),
                path: place.to_owned(),
            }),
        };
        match queued.runs.last_mut() {
            Some((last, count)) if *last == action => *count = count.saturating_add(1),
            _ => queued.runs.push((action, 1)),
        }
    }
}
