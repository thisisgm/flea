//! The one menu every face opens through: its card, its flyout, the cursor, keys, pointer, wheel and a choice's check.
use super::lines::{self, Built, Env};
use flea::ui_logic_mn1::{
    fit::Rect,
    places,
    refresh::{self, Refusal, EMPTY_CLIPBOARD},
    rows::{self, Lone, Submenu},
    Entry,
};
use gpui::Window;

mod card;
mod choice;
pub mod fade;
mod pointer;
pub use card::Card;

/// Which menu is open; only a listing's menus hold a selection that can move under them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Face {
    #[default]
    File,
    Background,
    Trash,
    Header,
    Rail,
    Place,
    Locked,
    Tab,
    Text,
}

impl Face {
    fn holds_selection(self) -> bool {
        matches!(self, Self::File | Self::Background)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Redraw,
    // The parent action, and the leaf's id for a flyout choice.
    Fire { action: String, leaf: String },
    // A catalog key the pane says out loud; the menu has closed.
    Refused(&'static str),
    // The menu closed and the pane takes keyboard focus back.
    Closed,
}

pub struct Flyout {
    // The row it hangs from, None for a lone flyout c or P opened over a hidden row.
    pub parent: Option<usize>,
    pub action: String,
    pub card: Card,
}

#[derive(Default)]
pub struct Menu {
    pub face: Face,
    pub card: Option<Card>,
    pub flyout: Option<Flyout>,
    pub faded: Option<fade::Faded>,
    pub fades: fade::Fades,
    // The selection the menu opened over, which a choice must still find.
    pub identity: String,
    // The handle a rail or Places menu acts on, such as place:2:/home/gm/Work.
    pub handle: String,
    pub work: Rect,
    // The listing is a history such as Recent, so a lone Copy As flyout hides Relative Path as the drawn row's does.
    pub history: bool,
    // Where the menu was asked to open, which every later fit places it from again.
    at: (f32, f32),
}

impl Menu {
    pub fn opened(&self) -> bool {
        self.card.is_some()
    }

    /// Where the menu was asked to open, before the fit moves its frame.
    #[cfg(test)]
    pub fn anchor(&self) -> (f32, f32) {
        self.at
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open(
        &mut self,
        face: Face,
        entries: Vec<Entry>,
        at: (f32, f32),
        work: Rect,
        identity: &str,
        env: &Env,
        window: &Window,
    ) -> Outcome {
        self.close();
        self.faded = None;
        if entries.is_empty() {
            return Outcome::Nothing;
        }
        let built = lines::build(&entries, env, window, 0., work.w);
        let metrics = built.metrics;
        let width = metrics.fit_width(built.widest, work.w);
        let frame = metrics.place(at, width, metrics.content_height(&entries), work, env.inset);
        // Every open lands on the first live row, the pointer's included, as v0.3.12 place() does.
        let cursor = Card::first(&entries);
        self.card = Some(Card::new(entries, built, frame, cursor));
        self.fades.closing = None;
        self.face = face;
        self.identity = identity.to_owned();
        self.work = work;
        self.at = at;
        Outcome::Redraw
    }

    /// The Locked tile's menu, or the Menus switch sentence when every row it could offer is hidden.
    #[allow(clippy::too_many_arguments)]
    pub fn open_locked(
        &mut self,
        path: &str,
        mode: u32,
        hidden: &[String],
        at: (f32, f32),
        work: Rect,
        env: &Env,
        window: &Window,
    ) -> Outcome {
        if let Some(refusal) = places::locked_refusal(mode, hidden) {
            self.close();
            return Outcome::Refused(refusal);
        }
        let outcome = self.open(
            Face::Locked,
            places::locked_entries(mode, hidden),
            at,
            work,
            "",
            env,
            window,
        );
        self.handle = path.to_owned();
        outcome
    }

    pub fn close(&mut self) -> Outcome {
        let was = self.opened();
        self.fades.closing = self.card.take().or(self.fades.closing.take());
        self.retire_flyout();
        self.handle.clear();
        if was {
            Outcome::Closed
        } else {
            Outcome::Nothing
        }
    }

    // However it opens, a flyout lands on its first live leaf and takes the keys, as v0.3.12 openSubmenu() does.
    fn open_flyout(
        &mut self,
        parent: Option<usize>,
        action: &str,
        leaves: Vec<Entry>,
        env: &Env,
        window: &Window,
    ) {
        let Some(card) = &self.card else { return };
        if leaves.is_empty() {
            return;
        }
        let built = lines::build(&leaves, env, window, card.frame.w, self.work.w);
        let metrics = built.metrics;
        let width = metrics.flyout_width(card.frame.w, built.widest, self.work.w);
        let top = parent.map_or(card.frame.y + metrics.pad as f32, |row| {
            card.row_screen_top(row)
        });
        let frame = metrics.place_flyout_insets(
            card.frame,
            width,
            metrics.content_height(&leaves),
            top,
            self.work,
            (env.horizontal_inset(), env.inset),
        );
        let cursor = Card::first(&leaves);
        let mut fly = Card::new(leaves, built, frame, cursor);
        // A view trimmed to a half row keeps its foot where the work area put it, level with a parent that reaches it.
        fly.frame.y = metrics
            .place_flyout_insets(
                card.frame,
                width,
                fly.view_h,
                top,
                self.work,
                (env.horizontal_inset(), env.inset),
            )
            .y;
        // Later cursor moves settle inside the room left under that top, so the foot row never runs off the work area.
        fly.full_h = fly
            .full_h
            .min((self.work.y + self.work.h - env.inset - fly.frame.y) as i32 - 2 * metrics.pad);
        fly.settle();
        self.faded = None;
        self.flyout = Some(Flyout {
            parent,
            action: action.to_owned(),
            card: fly,
        });
    }

    /// c or P with the menu closed or open: the drawn row's flyout, a lone one over a hidden row, or a refusal.
    pub fn open_submenu_for(
        &mut self,
        action: &str,
        clipboard: bool,
        env: &Env,
        window: &Window,
    ) -> Outcome {
        let Some(card) = &self.card else {
            return Outcome::Nothing;
        };
        match rows::submenu_for(action, &card.entries, clipboard) {
            Submenu::None => Outcome::Nothing,
            Submenu::Refuse => {
                self.close();
                Outcome::Refused(if action == "pasteAs" {
                    EMPTY_CLIPBOARD
                } else {
                    Refusal::Unavailable.key()
                })
            }
            Submenu::Row(row) => {
                let leaves = card.entries[row].submenu.clone().unwrap_or_default();
                if let Some(card) = &mut self.card {
                    card.cursor = Some(row);
                    card.settle();
                }
                self.open_flyout(Some(row), action, leaves, env, window);
                Outcome::Redraw
            }
            Submenu::Lone => {
                // No main row is its parent, so none stays lit beside it, as v0.3.12 lights only the leaf.
                if let Some(card) = &mut self.card {
                    card.cursor = None;
                }
                self.open_flyout(
                    None,
                    action,
                    rows::flyout_leaves(action, self.history),
                    env,
                    window,
                );
                Outcome::Redraw
            }
        }
    }

    /// A menu-context action from the key table.
    pub fn act(
        &mut self,
        action: &str,
        live: Option<&[Entry]>,
        identity_now: &str,
        env: &Env,
        window: &Window,
    ) -> Outcome {
        if !self.opened() {
            return Outcome::Nothing;
        }
        let in_flyout = self.flyout.is_some();
        match action {
            "escape" if self.flyout.is_some() => self.close_flyout(),
            "escape" => self.close(),
            "parent"
                if self.flyout.is_some()
                    && self.flyout.as_ref().is_some_and(|f| f.parent.is_some()) =>
            {
                self.close_flyout()
            }
            "cursorDown" | "focusNext" => self.step(in_flyout, 1),
            "cursorUp" | "focusPrevious" => self.step(in_flyout, -1),
            "menuRight" => self.enter_row(env, window),
            "open" | "preview" => self.choose(live, identity_now, env, window),
            _ => self.letter(action, live, identity_now),
        }
    }

    fn close_flyout(&mut self) -> Outcome {
        if self
            .flyout
            .as_ref()
            .is_some_and(|flyout| flyout.parent.is_none())
        {
            return self.close();
        }
        if self.flyout.is_none() {
            return Outcome::Nothing;
        }
        self.retire_flyout();
        Outcome::Redraw
    }

    fn step(&mut self, in_flyout: bool, delta: isize) -> Outcome {
        if !in_flyout {
            self.retire_flyout();
        }
        match (in_flyout, &mut self.flyout, &mut self.card) {
            (true, Some(flyout), _) => flyout.card.step(delta),
            (_, _, Some(card)) => {
                card.step(delta);
            }
            _ => return Outcome::Nothing,
        }
        Outcome::Redraw
    }

    fn enter_row(&mut self, env: &Env, window: &Window) -> Outcome {
        let Some(card) = &self.card else {
            return Outcome::Nothing;
        };
        let Some(row) = card.cursor else {
            return Outcome::Nothing;
        };
        let entry = &card.entries[row];
        if entry.disabled || !entry.has_submenu() {
            return Outcome::Nothing;
        }
        let (action, leaves) = (
            entry.action.to_string(),
            entry.submenu.clone().unwrap_or_default(),
        );
        self.open_flyout(Some(row), &action, leaves, env, window);
        Outcome::Redraw
    }

    /// A provider answer or a face change under the open menu: unchanged rows in the same face rebuild nothing, a change keeps the cursor's action.
    pub fn refresh(&mut self, entries: Vec<Entry>, env: &Env, window: &Window) -> bool {
        let Some(card) = &self.card else { return false };
        if card.entries == entries && card.built.typeface == *env.typeface {
            return false;
        }
        let lone = self
            .flyout
            .as_ref()
            .filter(|f| f.parent.is_none())
            .map(|f| (f.action.clone(), f.card.cursor));
        let (sub_row, sub_cursor) = self
            .flyout
            .as_ref()
            .map_or((None, 0), |f| (f.parent, f.card.cursor.unwrap_or(0)));
        let kept = refresh::refreshed_cursor(
            &card.entries,
            &entries,
            card.cursor.unwrap_or(0),
            sub_row,
            sub_cursor,
        );
        let pointer = card.cursor.is_none();
        let built = lines::build(&entries, env, window, 0., self.work.w);
        let metrics = built.metrics;
        let width = metrics.fit_width(built.widest, self.work.w);
        // A new width or height places the card again from its opening point, as v0.3.12 clampFrame does.
        let frame = metrics.place(
            self.at,
            width,
            metrics.content_height(&entries),
            self.work,
            env.inset,
        );
        let content_y = card.content_y;
        let mut next = Card::new(entries, built, frame, (!pointer).then_some(kept.cursor));
        next.content_y = content_y;
        next.settle();
        self.card = Some(next);
        match (lone, kept.submenu_row) {
            (Some((action, cursor)), _) => {
                self.open_flyout(
                    None,
                    &action,
                    rows::flyout_leaves(&action, self.history),
                    env,
                    window,
                );
                if let Some(flyout) = &mut self.flyout {
                    flyout.card.cursor = cursor;
                    flyout.card.settle();
                }
            }
            (None, Some(row)) => {
                let leaves = self
                    .card
                    .as_ref()
                    .and_then(|c| c.entries[row].submenu.clone())
                    .unwrap_or_default();
                let action = self
                    .card
                    .as_ref()
                    .map(|c| c.entries[row].action.to_string())
                    .unwrap_or_default();
                self.open_flyout(Some(row), &action, leaves, env, window);
                if let Some(flyout) = &mut self.flyout {
                    flyout.card.cursor = Some(kept.submenu_cursor);
                    flyout.card.settle();
                }
            }
            (None, None) => self.retire_flyout(),
        }
        true
    }
}
