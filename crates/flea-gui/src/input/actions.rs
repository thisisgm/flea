use crate::{list::pane::Move, state::Shell};
use gpui::{Context, Window};

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
    pub(crate) fn input_action(
        &mut self,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.input_blocked(cx) {
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
            let forward = action == "focusNext";
            let current = self.focus_view(window, cx);
            let dual = self.view == crate::window::startup::View::Dual;
            if dual
                && current == "list"
                && ((forward && self.focus_side == 0) || (!forward && self.focus_side == 1))
            {
                self.focus_side = 1 - self.focus_side;
                window.focus(&self.focus, cx);
            } else if let Some(stop) = self.input.step(current, forward) {
                window.focus(&stop.handle, cx);
                self.keyboard.context = stop.context;
                if stop.name == "rail" {
                    self.focus_side = 0;
                }
            } else {
                self.focus_side = usize::from(dual && !forward);
                window.focus(&self.focus, cx);
                self.keyboard.context = flea::keys::Context::Listing;
            }
            self.keyboard.disarm();
            cx.notify();
            return true;
        }
        if action == "escape" && !matches!(self.focus_view(window, cx), "" | "list") {
            window.focus(&self.focus, cx);
            self.keyboard.context = flea::keys::Context::Listing;
            cx.notify();
            return true;
        }
        if self.keyboard.context != flea::keys::Context::Listing {
            return false;
        }
        let climbs = action == "escape" && self.escape_climbs();
        let Some(host) = &mut self.panes[self.focus_side] else {
            return false;
        };
        let step = match action {
            "cursorUp" | "extendUp" => Some(Move::Up),
            "cursorDown" | "extendDown" => Some(Move::Down),
            "pageUp" => Some(Move::PageUp),
            "pageDown" => Some(Move::PageDown),
            "cursorFirst" => Some(Move::Home),
            "cursorLast" => Some(Move::End),
            _ => None,
        };
        if let Some(step) = step {
            let from = host.pane.cursor;
            let before_scroll = host.pane.scroll_y;
            if let Some(frame) = host.input.frame {
                if host.pane.busy() || frame.total == 0 {
                    return true;
                }
                let at = host.input.view_index(from);
                let columns = frame.columns.max(1);
                let page = ((frame.viewport() / frame.row).ceil() as usize / 2).max(1) * columns;
                let to = match step {
                    Move::Up => at.saturating_sub(columns),
                    Move::Down => at.saturating_add(columns),
                    Move::PageUp => at.saturating_sub(page),
                    Move::PageDown => at.saturating_add(page),
                    Move::Home => 0,
                    Move::End => frame.total - 1,
                }
                .min(frame.total - 1);
                let wrap =
                    flea::ui_logic_st1::pages::is_true(&self.launch.settings.data, &["wrapAtEnds"]);
                let to = match (wrap, action, at, frame.total) {
                    (true, "cursorDown", at, total) if at + 1 == total => 0,
                    (true, "cursorUp", 0, total) => total - 1,
                    _ => to,
                };
                host.pane.cursor = host.input.index(to);
            } else if !host.pane.move_cursor(step) {
                return true;
            }
            if let Some(mut frame) = host.input.frame {
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
            self.arm_input_tick(self.focus_side, window, cx);
            cx.notify();
            return true;
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
