//! The shortcut sheet's keyboard, decided once per open: every cap's legends and what each cap does under each modifier set.
//!
//! The host fills it from the live key table, so a held modifier or a looked-up chord reads the dispatcher's own answer.
//! Nothing here runs an action: the keyboard only says what one would be.
use crate::keys::physical::{self, Cap, Sample};
use crate::keys::{ALT, SHEET_GROUPS};

/// Every modifier set a keyboard is asked about: the four modifiers in each combination.
pub const MASKS: std::ops::Range<u8> = 0..16;

/// How a cap draws: lit while held or looked up, bound when it does something under the held modifiers, else faint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Faint,
    Bound,
    Lit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub cap: Cap,
    pub legend: String,
    pub shifted: String,
    /// The modifier bit this cap holds, lit while that modifier is down; right Alt holds Alt only where it is Alt.
    pub holds: u8,
}

/// What a cap does under one modifier set, as the dispatcher would run it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bound {
    pub cap: usize,
    pub mask: u8,
    pub action: String,
    pub label: String,
}

impl Bound {
    /// The first key of a two-key sequence, such as vim's `d d`: its action is the sequence's arm.
    pub fn pair(&self) -> bool {
        crate::keys::action_group(&self.action) != self.action
    }
}

/// What a host's lookup answers for one cap and modifier set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub action: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SheetKeyboard {
    pub faces: Vec<Face>,
    pub bound: Vec<Bound>,
}

impl SheetKeyboard {
    /// The keyboard for the active group, asking `find` what each bindable cap does under each modifier set.
    pub fn new(samples: &[Sample], mut find: impl FnMut(&Cap, u8) -> Option<Found>) -> Self {
        let caps = physical::caps(physical::is_iso(samples));
        let alt = physical::right_alt_is_alt(samples);
        let mut bound = Vec::new();
        for (index, cap) in caps.iter().enumerate() {
            if physical::inert(cap) || caps[..index].iter().any(|earlier| earlier.id == cap.id) {
                continue;
            }
            for mask in MASKS {
                if let Some(Found { action, label }) = find(cap, mask) {
                    bound.push(Bound {
                        cap: index,
                        mask,
                        action,
                        label,
                    });
                }
            }
        }
        let faces = caps
            .into_iter()
            .map(|cap| {
                let (mut legend, shifted) = physical::legends(&cap, samples);
                let mut holds = physical::modifier(&cap);
                if cap.id == "RALT" && alt {
                    legend = "ALT".into();
                    holds = ALT;
                }
                Face {
                    cap,
                    legend,
                    shifted,
                    holds,
                }
            })
            .collect();
        Self { faces, bound }
    }

    /// Each cap's look with `held` down and, while a chord is looked up, its cap.
    pub fn looks(&self, held: u8, chord: Option<usize>) -> Vec<Look> {
        let live: Vec<usize> = self
            .bound
            .iter()
            // Every binding that uses all the held set, as its list shows them: at rest every bound key.
            .filter(|bound| bound.mask & held == held)
            .map(|bound| bound.cap)
            .collect();
        (0..self.faces.len())
            .map(|index| {
                let holds = self.faces[index].holds;
                let held_cap = holds & held != 0;
                // A modifier is bound where pressing it too still reaches a binding, as its held list would show.
                let next = held | holds;
                let adds = holds != 0 && self.bound.iter().any(|bound| bound.mask & next == next);
                if held_cap || chord.is_some_and(|chord| self.same(chord, index)) {
                    Look::Lit
                } else if adds || live.iter().any(|cap| self.same(*cap, index)) {
                    Look::Bound
                } else {
                    Look::Faint
                }
            })
            .collect()
    }

    /// Each cap's look while a search or the pointer highlights `action`: every key that runs it lit, with every
    /// modifier any of its bindings holds, as its row lists them all.
    pub fn looks_for(&self, action: &str) -> Vec<Look> {
        let runs: Vec<&Bound> = self
            .bound
            .iter()
            .filter(|bound| crate::keys::action_group(&bound.action) == action)
            .collect();
        if runs.is_empty() {
            return self.looks(0, None);
        }
        let held = runs.iter().fold(0, |held, bound| held | bound.mask);
        (0..self.faces.len())
            .map(|index| {
                let lit = self.faces[index].holds & held != 0
                    || runs.iter().any(|bound| self.same(bound.cap, index));
                if lit {
                    Look::Lit
                } else {
                    Look::Faint
                }
            })
            .collect()
    }

    /// The rows a held set lists: every binding that uses all of it, in the rest list's groups, those outside them last.
    /// Within a group the exact set comes first, then by how many more it adds. A looked-up chord narrows them to its
    /// cap, its own binding's group first.
    pub fn rows(&self, held: u8, chord: Option<usize>) -> Vec<&Bound> {
        let mut rows: Vec<&Bound> = self
            .bound
            .iter()
            .filter(|bound| bound.mask & held == held)
            .filter(|bound| chord.map_or(true, |chord| self.same(chord, bound.cap)))
            .collect();
        let group = |bound: &Bound| group_of(&bound.action).unwrap_or(SHEET_GROUPS.len());
        let exact = chord.and_then(|_| rows.iter().find(|bound| bound.mask == held));
        let lead = exact.map(|bound| group(bound));
        // Within one set the rows read the keyboard from the top left, as the caps they name are drawn.
        rows.sort_by(|a, b| {
            let place = |bound: &Bound| {
                let cap = &self.faces[bound.cap].cap;
                (
                    lead.is_some_and(|lead| group(bound) != lead),
                    group(bound),
                    (bound.mask ^ held).count_ones(),
                    bound.mask,
                    cap.row,
                    cap.x,
                )
            };
            place(a)
                .partial_cmp(&place(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        rows
    }

    /// The cap a key event names, by its US position or its key name, so a chord on any group finds its cap.
    pub fn cap_of(&self, key: &str, us: Option<&str>) -> Option<usize> {
        if let Some(us) = us.filter(|us| !us.is_empty()) {
            // With Shift held the position reads as the US key's shifted character, `&` for 7; the ISO key beside
            // Shift and `.` both shift to `>`, so the live legend the key typed decides between them.
            let at = |index: &usize| {
                let cap = &self.faces[*index].cap;
                cap.us == us || us_shifted(cap.us) == Some(us)
            };
            let found: Vec<usize> = (0..self.faces.len()).filter(at).collect();
            if let Some(&first) = found.first() {
                let typed = |index: &usize| {
                    let face = &self.faces[*index];
                    !key.is_empty() && (face.shifted == key || face.legend == key)
                };
                return Some(found.into_iter().find(typed).unwrap_or(first));
            }
        }
        // Without a reported position, as for the ISO key beside left Shift, a key is found by what it types on this
        // layout, then by its name or the US character it names.
        let usable = |face: &Face| !physical::inert(&face.cap) && !key.is_empty();
        let typed = |face: &Face| face.legend.to_lowercase() == key || face.shifted == key;
        let printing = |face: &Face| usable(face) && !face.cap.us.is_empty() && typed(face);
        let named = |face: &Face| usable(face) && (face.cap.key == key || face.cap.us == key);
        let faces = || self.faces.iter();
        faces()
            .position(printing)
            .or_else(|| faces().position(named))
    }

    /// Two faces are one key when they share a position, as ISO Enter's two pieces do.
    fn same(&self, a: usize, b: usize) -> bool {
        a == b || self.faces[a].cap.id == self.faces[b].cap.id
    }
}

#[cfg(test)]
#[path = "sheet_keyboard_tests.rs"]
mod tests;

mod groups;
mod words;
pub use groups::group_of;
use words::us_shifted;
pub use words::{canonical, held_words, spell};
