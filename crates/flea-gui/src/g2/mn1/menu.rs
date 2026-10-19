//! The one menu every face opens through: its card, its flyout, the cursor, keys, pointer, wheel and a choice's check.
use super::lines::{self, Built, Env};
use flea::ui_logic_mn1::{
    fit::{self, Rect, PAD},
    places,
    refresh::{self, validate_choice, Refusal, EMPTY_CLIPBOARD},
    rows::{self, Lone, Submenu},
    Entry,
};
use gpui::Window;

mod pointer;

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

pub struct Card {
    pub entries: Vec<Entry>,
    pub built: Built,
    pub frame: Rect,
    pub content_y: i32,
    pub view_h: i32,
    // The view the work area allows before a half-row trim.
    full_h: i32,
    pub cursor: Option<usize>,
    // The wheel's and the touchpad's leftover travel, one each per card, as each v0.3.12 CardScroll keeps its own.
    notch: i32,
    touch: f32,
}

impl Card {
    fn new(entries: Vec<Entry>, built: Built, frame: Rect, cursor: Option<usize>) -> Self {
        let full_h = frame.h as i32 - 2 * PAD;
        let mut card = Self {
            entries,
            built,
            view_h: full_h,
            full_h,
            frame,
            content_y: 0,
            cursor,
            notch: 0,
            touch: 0.,
        };
        card.settle();
        card
    }

    fn first(entries: &[Entry]) -> Option<usize> {
        usize::try_from(rows::step_row(entries, -1, 1)).ok()
    }

    // The scroll a cursor move lands on, and the view trimmed so a clipped edge cuts a row at half.
    fn settle(&mut self) {
        let full = self.full_h;
        let (y, h) = match self.cursor {
            Some(cursor) => fit::scroll_for(&self.entries, cursor, self.content_y, full),
            None => fit::menu_cut(&self.entries, self.content_y, full, None),
        };
        self.content_y = y;
        self.view_h = h;
        self.frame.h = (h + 2 * PAD) as f32;
    }

    fn step(&mut self, delta: isize) {
        let from = self.cursor.map_or(
            if delta > 0 {
                -1
            } else {
                self.entries.len() as isize
            },
            |at| at as isize,
        );
        if let Ok(at) = usize::try_from(rows::step_row(&self.entries, from, delta)) {
            if at < self.entries.len() {
                self.cursor = Some(at);
            }
        }
        self.settle();
    }

    pub fn row_screen_top(&self, index: usize) -> f32 {
        self.frame.y + (PAD + fit::row_top(&self.entries, index) - self.content_y) as f32
    }

    pub fn row_at(&self, x: f32, y: f32) -> Option<usize> {
        let f = self.frame;
        if x < f.x
            || x >= f.x + f.w
            || y < f.y + PAD as f32
            || y >= f.y + (PAD + self.view_h) as f32
        {
            return None;
        }
        let content = (y - f.y) as i32 - PAD + self.content_y;
        let mut top = 0;
        for (index, entry) in self.entries.iter().enumerate() {
            top += fit::height(entry);
            if content < top {
                return entry.steppable().then_some(index);
            }
        }
        None
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        let f = self.frame;
        x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h
    }
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
    // The selection the menu opened over, which a choice must still find.
    pub identity: String,
    // The handle a rail or Places menu acts on, such as place:2:/home/gm/Work.
    pub handle: String,
    pub work: Rect,
    // Where the menu was asked to open, which every later fit places it from again.
    at: (f32, f32),
}

impl Menu {
    pub fn opened(&self) -> bool {
        self.card.is_some()
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
        if entries.is_empty() {
            return Outcome::Nothing;
        }
        let built = lines::build(&entries, env, window);
        let width = fit::fit_width(built.widest, work.w);
        let frame = fit::place(at, width, fit::content_height(&entries), work, env.inset);
        // Every open lands on the first live row, the pointer's included, as v0.3.12 place() does.
        let cursor = Card::first(&entries);
        self.card = Some(Card::new(entries, built, frame, cursor));
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
        self.card = None;
        self.flyout = None;
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
        let built = lines::build(&leaves, env, window);
        let width = fit::flyout_width(card.frame.w, built.widest, self.work.w);
        let top = parent.map_or(card.frame.y + PAD as f32, |row| card.row_screen_top(row));
        let frame = fit::place_flyout(
            card.frame,
            width,
            fit::content_height(&leaves),
            top,
            self.work,
            env.inset,
        );
        let cursor = Card::first(&leaves);
        let mut fly = Card::new(leaves, built, frame, cursor);
        // A view trimmed to a half row keeps its foot where the work area put it, level with a parent that reaches it.
        fly.frame.y = fit::place_flyout(card.frame, width, fly.view_h, top, self.work, env.inset).y;
        // Later cursor moves settle inside the room left under that top, so the foot row never runs off the work area.
        fly.full_h = fly
            .full_h
            .min((self.work.y + self.work.h - env.inset - fly.frame.y) as i32 - 2 * PAD);
        fly.settle();
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
                self.open_flyout(None, action, rows::flyout_entries(action), env, window);
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
        match self.flyout.take() {
            // A lone flyout is the whole menu, so closing it closes the menu.
            Some(Flyout { parent: None, .. }) => self.close(),
            Some(_) => Outcome::Redraw,
            None => Outcome::Nothing,
        }
    }

    fn step(&mut self, in_flyout: bool, delta: isize) -> Outcome {
        match (in_flyout, &mut self.flyout, &mut self.card) {
            (true, Some(flyout), _) => flyout.card.step(delta),
            (_, _, Some(card)) => {
                card.step(delta);
                self.flyout = None;
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

    fn moved(&self, identity_now: &str) -> bool {
        self.face.holds_selection() && identity_now != self.identity
    }

    fn fire(
        &mut self,
        live: Option<&[Entry]>,
        action: String,
        leaf: String,
        identity_now: &str,
    ) -> Outcome {
        let rows = live.map_or_else(
            || {
                self.card
                    .as_ref()
                    .map(|c| c.entries.clone())
                    .unwrap_or_default()
            },
            <[Entry]>::to_vec,
        );
        let checked = validate_choice(&rows, &action, &leaf, self.moved(identity_now));
        self.close();
        match checked {
            Ok(()) => Outcome::Fire { action, leaf },
            Err(refusal) => Outcome::Refused(refusal.key()),
        }
    }

    fn choose(
        &mut self,
        live: Option<&[Entry]>,
        identity_now: &str,
        env: &Env,
        window: &Window,
    ) -> Outcome {
        if let Some(flyout) = &self.flyout {
            let Some(leaf) = flyout
                .card
                .cursor
                .map(|at| flyout.card.entries[at].id.to_string())
            else {
                return Outcome::Nothing;
            };
            return self.choose_leaf(&leaf, live, identity_now);
        }
        let Some(card) = &self.card else {
            return Outcome::Nothing;
        };
        let Some(row) = card.cursor else {
            return Outcome::Nothing;
        };
        if card.entries[row].has_submenu() {
            return self.enter_row(env, window);
        }
        let action = card.entries[row].action.to_string();
        self.fire(live, action, String::new(), identity_now)
    }

    fn choose_leaf(&mut self, leaf: &str, live: Option<&[Entry]>, identity_now: &str) -> Outcome {
        let Some(flyout) = &self.flyout else {
            return Outcome::Nothing;
        };
        let action = flyout.action.clone();
        if flyout.parent.is_some() {
            return self.fire(live, action, leaf.to_owned(), identity_now);
        }
        let for_rail = matches!(self.face, Face::Rail | Face::Place);
        let lone = rows::lone_choice(
            &action,
            leaf,
            for_rail,
            self.face == Face::Header,
            self.face.holds_selection(),
            &self.identity,
            identity_now,
        );
        self.close();
        match lone {
            Lone::Moved => Outcome::Refused(Refusal::Moved.key()),
            Lone::Unknown => Outcome::Refused(Refusal::Unavailable.key()),
            Lone::Fire(_) => Outcome::Fire {
                action,
                leaf: leaf.to_owned(),
            },
        }
    }

    // A flyout leaf's own letter, or a tag digit for the Tags flyout.
    fn letter(&mut self, action: &str, live: Option<&[Entry]>, identity_now: &str) -> Outcome {
        if let Some(flyout) = &self.flyout {
            if flyout
                .card
                .entries
                .iter()
                .any(|leaf| leaf.id == action && leaf.steppable())
            {
                return self.choose_leaf(action, live, identity_now);
            }
        }
        let Some(slot) = action
            .strip_prefix("tag")
            .and_then(|n| n.parse::<u8>().ok())
        else {
            return Outcome::Nothing;
        };
        let tags = self.card.as_ref().and_then(|card| {
            card.entries
                .iter()
                .find(|e| e.action == "tags" && !e.disabled)
        });
        let Some(leaf) = tags
            .and_then(|row| row.submenu.as_ref())
            .and_then(|leaves| leaves.iter().find(|l| l.tag == Some(slot)))
        else {
            return Outcome::Nothing;
        };
        let leaf = leaf.id.to_string();
        self.fire(live, "tags".into(), leaf, identity_now)
    }

    /// A provider answer under the open menu: unchanged rows rebuild nothing, a change keeps the cursor's action.
    pub fn refresh(&mut self, entries: Vec<Entry>, env: &Env, window: &Window) -> bool {
        let Some(card) = &self.card else { return false };
        if card.entries == entries {
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
        let built = lines::build(&entries, env, window);
        let width = fit::fit_width(built.widest, self.work.w);
        // A new width or height places the card again from its opening point, as v0.3.12 clampFrame does.
        let frame = fit::place(
            self.at,
            width,
            fit::content_height(&entries),
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
                self.open_flyout(None, &action, rows::flyout_entries(&action), env, window);
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
            (None, None) => self.flyout = None,
        }
        true
    }
}
