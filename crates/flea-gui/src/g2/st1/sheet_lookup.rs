//! The sheet's drawn keyboard state: held modifiers, looked-up chords, and the even column split beside a keyboard.
use super::super::sheet_view::HEAD_SLOT;
use super::Sheet;
use crate::g2::mn1::keycaps;
use crate::locale_geometry::isolate_ltr;
use flea::keys::display::KeycapSnapshot;
use flea::keys::{action_group, presentation, SheetGroup, DIGITS, SHEET_GROUPS};
use flea::ui_logic_st1::sheet::Candidate;
use flea::ui_logic_st1::sheet_keyboard::{canonical, group_of, spell, SheetKeyboard};
use flea::ui_logic_st1::Text;

/// A list row under the pointer, whose keys the keyboard lights: a rest row's action, or a held row's binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hover {
    Action(String),
    Binding { mask: u8, cap: usize },
}

pub(super) type Grouped<'a> = (usize, &'static SheetGroup, Vec<&'a presentation::SheetRow>);

/// Moves the column break to the group boundary where the two columns' rows differ least.
pub(super) fn balance(groups: &mut [Grouped]) {
    let total: usize = groups.iter().map(|(_, _, rows)| rows.len()).sum();
    let mut left = 0;
    let mut best = (usize::MAX, groups.len());
    for index in 1..groups.len() {
        left += groups[index - 1].2.len();
        let gap = left.abs_diff(total - left);
        if gap < best.0 {
            best = (gap, index);
        }
    }
    for (index, group) in groups.iter_mut().enumerate() {
        group.0 = usize::from(index >= best.1);
    }
}

impl Sheet {
    /// The modifiers now held; releasing any of them ends a looked-up chord, which named the set it was pressed with.
    pub fn hold(&mut self, held: u8) {
        let released = self.held & !held != 0;
        // Another held set redraws the list under the pointer, so the hovered row is gone.
        if held != self.held {
            self.hover = None;
            // Another held list starts at its top; a search keeps its place as Shift types a capital.
            if !self.typing {
                self.scroll = 0.;
            }
        }
        self.held = held;
        if (held == 0 || released) && self.pressed.is_none() {
            self.chord = None;
        }
    }

    /// Nothing is held any more: the window lost the keys, so no look-up or held list stays behind.
    pub fn forget(&mut self) {
        self.held = 0;
        self.chord = None;
        self.pressed = None;
        if !self.typing {
            self.scroll = 0.;
        }
    }

    /// A key let go: the plain key that looked itself up ends its look-up. True when it did.
    pub fn release(&mut self, key: &str) -> bool {
        if self.pressed.as_deref() != Some(key) {
            return false;
        }
        self.pressed = None;
        self.chord = None;
        true
    }

    /// A rebuilt keyboard replaces the old one; a looked-up chord or hovered binding named a cap of the old one, so it
    /// ends. Its bindings are what the search finds, so the query is ranked again.
    pub fn set_keyboard(&mut self, keyboard: Option<SheetKeyboard>) {
        self.keyboard = keyboard;
        self.chord = None;
        self.pressed = None;
        self.hover = None;
        self.rerank();
    }

    /// Looks up a chord on the keyboard without running it: true when the key named a cap and the sheet took it.
    pub fn look_up(&mut self, key: &str, us: Option<&str>, held: u8) -> bool {
        let Some(cap) = self
            .keyboard
            .as_ref()
            .and_then(|keyboard| keyboard.cap_of(key, us))
        else {
            return false;
        };
        self.held = held;
        self.chord = Some(cap);
        // The looked-up chord replaces the list under the pointer, so no row stays lit from before.
        self.hover = None;
        true
    }

    /// The highlighted result was picked: a word narrowed the list, or the arrows or pointer moved onto it. The
    /// keyboard's search opens on every binding with none picked, and lights a row's keys only once one is.
    pub fn chosen(&self) -> bool {
        !self.query.is_empty() || self.browsed
    }

    /// Over a keyboard the results read in the rest list's groups: the group of the best match first, each group's
    /// rows in rank. The search opened on no words keeps the rest list's order.
    pub(super) fn group_results(&mut self) {
        if self.keyboard.is_none() {
            return;
        }
        let last = SHEET_GROUPS.len();
        let group = |row: &Candidate| group_of(&row.action).unwrap_or(last);
        if self.query.is_empty() {
            let actions = || SHEET_GROUPS.iter().flat_map(|group| group.actions.iter());
            let at = |row: &Candidate| actions().position(|action| *action == row.action);
            self.ranked
                .sort_by_key(|row| (group(row), at(row).unwrap_or(usize::MAX)));
            return;
        }
        let mut order: Vec<usize> = Vec::new();
        for row in &self.ranked {
            if !order.contains(&group(row)) {
                order.push(group(row));
            }
        }
        self.ranked
            .sort_by_key(|row| order.iter().position(|at| *at == group(row)));
    }

    /// Over a keyboard, the bindings the rest list has no row for, plugin and user keys among them, as search rows
    /// spelled the rest list's way.
    pub(super) fn unlisted(&self) -> Vec<Candidate> {
        let Some(keyboard) = self.keyboard.as_ref() else {
            return Vec::new();
        };
        let mut out: Vec<Candidate> = Vec::new();
        // A pair's arm reads as its action's row, and the digits' tabs are the rest list's to leave out.
        let listed = |action: &str| {
            let group = action_group(action);
            DIGITS.iter().any(|digit| digit.action == action)
                || self
                    .rows
                    .iter()
                    .any(|row| row.action == action || row.action == group)
        };
        for bound in keyboard.bound.iter().filter(|bound| !listed(&bound.action)) {
            let keys = canonical(&keyboard.faces[bound.cap], bound.mask);
            match out.iter_mut().find(|row| row.action == bound.action) {
                Some(row) => row.keys = format!("{} / {keys}", row.keys),
                None => out.push(Candidate {
                    label: Text::data(bound.label.clone()),
                    keys,
                    action: bound.action.clone(),
                    ..Default::default()
                }),
            }
        }
        out
    }

    /// The heading result `index` opens: over a keyboard, the group of each group's first result.
    pub fn result_head(&self, index: usize) -> Option<Option<usize>> {
        self.keyboard.as_ref()?;
        let group = |at: usize| group_of(&self.ranked[at].action);
        let first = index < self.ranked.len() && (index == 0 || group(index) != group(index - 1));
        first.then(|| group(index))
    }

    /// The headings' height over the first `count` results.
    pub(super) fn headed(&self, count: usize) -> f32 {
        let heads = (0..count.min(self.ranked.len()))
            .filter(|index| self.result_head(*index).is_some())
            .count();
        heads as f32 * HEAD_SLOT
    }
}

/// A row's keys as a list draws them: the group's keycaps, and over a keyboard the caps' own spelling.
pub(super) fn spelled(
    keys: &str,
    keycaps: Option<&KeycapSnapshot>,
    rtl: bool,
    keyboard: bool,
) -> String {
    if !keyboard {
        return keycaps::chord(keys, keycaps, rtl);
    }
    let words = spell(&keycaps::chord(keys, keycaps, false));
    if rtl {
        isolate_ltr(&words)
    } else {
        words
    }
}
