//! The status line's message rules: what the centre says, in which role, and what its muted lane carries beside it.
use crate::i18n::I18n;
use crate::keys::presentation::{hint_for, hinted};

pub use super::lane::{
    activity_changed, cancel_activity, displayed_starting, secondary_text, selection_bytes,
    Activity, RowSize, Walked,
};

/// The middot every lane joins its parts with.
pub const SEP: &str = " · ";
/// The key a standing error is dismissed with; the oracle names it outright rather than through a binding.
pub const DISMISS_KEY: &str = "esc";

/// A note the durable verdict carries, mirrored from src/backend/durable.rs and matched whole on the wire's `note`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Note {
    Background,
    Unconfirmed,
}

impl Note {
    pub fn from_wire(note: &str) -> Option<Self> {
        match note {
            "rclone uploads them in the background" => Some(Self::Background),
            "copied, but the drive did not confirm the folder" => Some(Self::Unconfirmed),
            _ => None,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Background => "feedback.verdict.background",
            Self::Unconfirmed => "feedback.verdict.unconfirmed",
        }
    }
}

/// The advice a result carries in the muted lane, typed so nothing is ever split back out of translated text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hint {
    #[default]
    None,
    Undo,
    /// A completion receipt's undo target leaves with its ordinary notice timer.
    TransientUndo,
    Paste,
    Deletes,
    Note(Note, bool),
    /// Carry's own advice, standing while the carry rides.
    Carry,
    /// A notice that names the key which clears it, as the locked-menu board's status line does.
    Dismiss,
}

/// One lane segment of a hint, the undo key kept apart so it can draw as its own target.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HintText {
    pub rest: String,
    pub undo: String,
}

impl Hint {
    /// The hint's words for the running preset, empty when the preset binds no key for it.
    pub fn text(self, catalog: &I18n, preset: &str) -> HintText {
        let undo = || hinted(catalog, "feedback.hint.undo", preset, "undo");
        match self {
            Self::None => HintText::default(),
            Self::Undo | Self::TransientUndo => HintText {
                rest: String::new(),
                undo: undo(),
            },
            Self::Paste => HintText {
                rest: hinted(catalog, "feedback.hint.paste", preset, "paste"),
                undo: String::new(),
            },
            Self::Deletes => HintText {
                rest: hinted(
                    catalog,
                    "feedback.hint.deletes",
                    preset,
                    "deletePermanently",
                ),
                undo: String::new(),
            },
            Self::Dismiss => HintText {
                rest: catalog.tr_args("feedback.hint.dismiss", &[&catalog.keycap(DISMISS_KEY)]),
                undo: String::new(),
            },
            Self::Note(note, with_undo) => HintText {
                rest: catalog.tr(note.key()),
                undo: if with_undo { undo() } else { String::new() },
            },
            Self::Carry => HintText {
                rest: catalog.tr("carry.hint"),
                undo: String::new(),
            },
        }
    }

    /// Whether the notice stands until replaced or withdrawn: an undo target, or a carry that still rides.
    pub fn stands(self, preset: &str) -> bool {
        self == Self::Carry || (self != Self::TransientUndo && self.has_undo(preset))
    }

    /// Whether the hint ends in the undo key, which makes that key a click target.
    pub fn has_undo(self, preset: &str) -> bool {
        matches!(self, Self::Undo | Self::TransientUndo | Self::Note(_, true))
            && !hint_for(preset, "undo").is_empty()
    }
}

impl HintText {
    pub fn is_empty(&self) -> bool {
        self.rest.is_empty() && self.undo.is_empty()
    }

    /// The whole hint as one key string, the undo key last.
    pub fn joined(&self) -> String {
        join_nonempty(&[&self.rest, &self.undo], SEP)
    }
}

/// A receipt's trailing undo advice draws in the muted target and expires on the ordinary notice clock.
pub fn split_undo(text: &str, catalog: &I18n, preset: &str) -> (String, Hint) {
    let tails = [
        crate::ui_logic_file_ops::undo_hint(catalog, preset),
        crate::ui_logic_rail_places::tags::undo_hint(catalog, preset),
    ];
    match tails
        .iter()
        .filter(|tail| !tail.is_empty())
        .find_map(|tail| text.strip_suffix(tail))
    {
        Some(sentence) => (sentence.to_owned(), Hint::TransientUndo),
        None => (text.to_owned(), Hint::None),
    }
}

fn join_nonempty(parts: &[&str], sep: &str) -> String {
    parts
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(sep)
}

/// What the centre has to choose from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Slot<'a> {
    pub transient: &'a str,
    pub transient_is_error: bool,
    pub arm_live: bool,
    pub searching: bool,
    pub search_line: &'a str,
    pub sticky_here: bool,
    pub sticky: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Foreground,
    Error,
}

pub fn error_here(slot: &Slot) -> bool {
    !slot.transient.is_empty() && slot.transient_is_error
}

pub fn prompt_here(slot: &Slot) -> bool {
    slot.arm_live
}

/// An unacknowledged error or a live prompt, then activity, then a running search, then the notice; idle is empty.
pub fn centre_text<'a>(slot: &Slot<'a>) -> &'a str {
    if error_here(slot) || prompt_here(slot) {
        slot.transient
    } else if slot.sticky_here {
        slot.sticky
    } else if slot.searching {
        slot.search_line
    } else {
        slot.transient
    }
}

pub fn centre_role(slot: &Slot) -> Role {
    if error_here(slot) {
        Role::Error
    } else {
        Role::Foreground
    }
}

/// Whether the centre draws the notice, whose display time the strip's timer ends.
pub fn notice_shown(slot: &Slot) -> bool {
    !prompt_here(slot)
        && !slot.transient.is_empty()
        && !slot.transient_is_error
        && !slot.sticky_here
        && !slot.searching
}

/// One standing error; `place` is set only for a no-Trash refusal, which belongs to where it was raised.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorEntry {
    /// An opaque receipt token; zero belongs to an untracked error.
    pub serial: u64,
    pub text: String,
    pub detail: String,
    pub hint: Hint,
    pub place: String,
}

/// What the transient lane draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shown {
    pub text: String,
    pub is_error: bool,
    pub detail: String,
}

/// A live arm's prompt, else the oldest unacknowledged error, else the notice.
pub fn transient_of(errors: &[ErrorEntry], notice: &str, prompt: &str, live: bool) -> Shown {
    if live {
        return Shown {
            text: prompt.into(),
            is_error: false,
            detail: String::new(),
        };
    }
    match errors.first() {
        Some(entry) => Shown {
            text: entry.text.clone(),
            is_error: true,
            detail: entry.detail.clone(),
        },
        None => Shown {
            text: notice.into(),
            is_error: false,
            detail: String::new(),
        },
    }
}

/// Queues an error; a placed refusal replaces its own copy in that place where it stands.
pub fn with_error(
    errors: &mut Vec<ErrorEntry>,
    text: &str,
    detail: &str,
    hint: Hint,
    place: Option<&str>,
) {
    let entry = ErrorEntry {
        serial: 0,
        text: text.into(),
        detail: detail.into(),
        hint,
        place: place.unwrap_or("").into(),
    };
    let at = if entry.place.is_empty() {
        None
    } else {
        errors
            .iter()
            .position(|e| e.text == entry.text && e.place == entry.place)
    };
    match at {
        Some(at) => errors[at] = entry,
        None => errors.push(entry),
    }
}

/// Leaving a place drops its refusals; returns whether anything was dropped.
pub fn errors_at(errors: &mut Vec<ErrorEntry>, place: &str) -> bool {
    let before = errors.len();
    errors.retain(|entry| entry.place.is_empty() || entry.place == place);
    errors.len() != before
}

/// An arm's owner and the stamp the owner set before its prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arm {
    pub owner: u64,
    pub stamp: u64,
}

/// Live while the bar still has that owner and the owner still holds that stamp.
pub fn arm_live(arm: Option<Arm>, owner: u64, owner_stamp: u64) -> bool {
    arm.is_some_and(|arm| arm.stamp > 0 && arm.owner == owner && owner_stamp == arm.stamp)
}

/// What the arm has left by its owner's clock.
pub fn arm_left(arm: Option<Arm>, now: u64, arm_ms: u64) -> i64 {
    arm.map_or(0, |arm| arm_ms as i64 - (now as i64 - arm.stamp as i64))
}
