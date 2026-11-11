//! The pointer and the wheel over an open menu: hover takes the cursor, a press chooses or closes, the wheel and touchpad step rows.
use super::super::lines::Env;
use super::{Card, Menu, Outcome};
use flea::ui_logic_context_menus::{fit, Entry};
use gpui::Window;

impl Menu {
    /// The pointer over the cards: the row under it takes the cursor, and a flyout parent opens its flyout lit.
    pub fn hover(&mut self, x: f32, y: f32, env: &Env, window: &Window) -> Outcome {
        if let Some(flyout) = &mut self.flyout {
            if flyout.card.contains(x, y) {
                let at = flyout.card.row_at(x, y);
                if at.is_some() && at != flyout.card.cursor {
                    flyout.card.cursor = at;
                    return Outcome::Redraw;
                }
                return Outcome::Nothing;
            }
        }
        let Some(card) = &mut self.card else {
            return Outcome::Nothing;
        };
        let Some(row) = card.row_at(x, y) else {
            return Outcome::Nothing;
        };
        if card.cursor == Some(row) && self.flyout.as_ref().is_none_or(|f| f.parent == Some(row)) {
            return Outcome::Nothing;
        }
        card.cursor = Some(row);
        let entry = &card.entries[row];
        let leaves = (!entry.disabled).then(|| entry.submenu.clone()).flatten();
        let action = entry.action.to_string();
        self.retire_flyout();
        if let Some(leaves) = leaves {
            self.open_flyout(Some(row), &action, leaves, env, window);
        }
        Outcome::Redraw
    }

    /// A press: a row chooses, the ground outside both cards closes the menu.
    pub fn click(
        &mut self,
        x: f32,
        y: f32,
        live: Option<&[Entry]>,
        identity_now: &str,
        env: &Env,
        window: &Window,
    ) -> Outcome {
        let in_flyout = self.flyout.as_ref().is_some_and(|f| f.card.contains(x, y));
        let in_card = self.card.as_ref().is_some_and(|c| c.contains(x, y));
        if !in_flyout && !in_card {
            return self.close();
        }
        let _ = self.hover(x, y, env, window);
        if in_flyout {
            let leaf = self.flyout.as_ref().and_then(|f| {
                f.card
                    .row_at(x, y)
                    .map(|at| f.card.entries[at].id.to_string())
            });
            return leaf.map_or(Outcome::Nothing, |leaf| {
                self.choose_leaf(&leaf, live, identity_now)
            });
        }
        if self.card.as_ref().and_then(|c| c.row_at(x, y)).is_none() {
            return Outcome::Nothing;
        }
        // Only the flyout's own parent keeps it open under the pointer; a press there opens it afresh, as v0.3.12 onActivated does.
        if self.flyout.is_some() {
            return self.enter_row(env, window);
        }
        self.choose(live, identity_now, env, window)
    }

    /// Raw wheel angle units over the card under the pointer, folded into whole rows that card's cursor steps.
    pub fn wheel(&mut self, x: f32, y: f32, units: i32) -> Outcome {
        let Some((in_flyout, card)) = self.card_under(x, y) else {
            return Outcome::Nothing;
        };
        let (steps, rest) = fit::notch_steps(card.notch, units);
        card.notch = rest;
        self.walk(in_flyout, steps)
    }

    /// Touchpad pixels over the card under the pointer, one row per row height; a gesture's start drops the last swipe's leftover.
    pub fn touchpad(&mut self, x: f32, y: f32, pixels: f32, begin: bool) -> Outcome {
        let Some((in_flyout, card)) = self.card_under(x, y) else {
            return Outcome::Nothing;
        };
        let (steps, rest) = fit::touch_steps(
            if begin { 0. } else { card.touch },
            pixels,
            card.built.metrics.row as f32,
        );
        card.touch = rest;
        self.walk(in_flyout, steps)
    }

    // The flyout or the menu card under the pointer; over the ground outside both the wheel is swallowed.
    fn card_under(&mut self, x: f32, y: f32) -> Option<(bool, &mut Card)> {
        if self.flyout.as_ref().is_some_and(|f| f.card.contains(x, y)) {
            return self.flyout.as_mut().map(|f| (true, &mut f.card));
        }
        self.card
            .as_mut()
            .filter(|c| c.contains(x, y))
            .map(|c| (false, c))
    }

    // A step over the flyout moves its cursor; one over the menu closes the flyout first, as v0.3.12 stepMain() does.
    fn walk(&mut self, in_flyout: bool, steps: i32) -> Outcome {
        if steps == 0 {
            return Outcome::Nothing;
        }
        for _ in 0..steps.unsigned_abs() {
            let _ = self.step(in_flyout, steps.signum() as isize);
        }
        Outcome::Redraw
    }
}
