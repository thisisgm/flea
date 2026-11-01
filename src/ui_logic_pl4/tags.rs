//! File tags: the seven names, selection states, the flyout and rail rows, and the status sentences.
use crate::i18n::I18n;
use crate::keys::presentation;

// Menu order, which is also the 1 to 7 key order; src/backend/tags.rs NAMES is the same list.
pub const NAMES: [&str; 7] = ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"];

// The board's tag swatch square, and the smaller one Properties draws beside 14 px text, in px.
pub const SWATCH_SIZE: f32 = 14.0;
pub const PAIR_SWATCH_SIZE: f32 = 10.0;
// The gap between a row's swatches and the fallback digit's frame width, in px.
pub const SWATCH_GAP: f32 = 3.0;
pub const FALLBACK_BORDER: f32 = 2.0;
// The fallback digit's bold text size, and the room a list row reserves for its marks, in px, as the tag-marks board states.
pub const DIGIT_TEXT: f32 = 10.0;
pub const ROW_SLOT: f32 = 97.0;

// Sample input: "Blue" answers 5; a name outside the seven answers None.
pub fn index_of(name: &str) -> Option<usize> {
    NAMES.iter().position(|known| *known == name)
}

// The digit each name answers to, "" when no digit names it.
pub fn key_for(name: &str) -> &'static str {
    const DIGITS: [&str; 7] = ["1", "2", "3", "4", "5", "6", "7"];
    index_of(name).map_or("", |at| DIGITS[at])
}

// The catalog key a tag's visible name resolves through; the English name stays the stored identity.
pub fn label_key(name: &str) -> &'static str {
    const KEYS: [&str; 7] = [
        "places.tag.red",
        "places.tag.orange",
        "places.tag.yellow",
        "places.tag.green",
        "places.tag.blue",
        "places.tag.purple",
        "places.tag.gray",
    ];
    index_of(name).map_or("", |at| KEYS[at])
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    Applied,
    Mixed,
    #[default]
    Absent,
}

// Across a selection: applied when every file carries it, mixed when some do, absent otherwise.
pub fn state_for<S: AsRef<str>>(files: &[Vec<S>], name: &str) -> State {
    let have = files
        .iter()
        .filter(|tags| tags.iter().any(|tag| tag.as_ref() == name))
        .count();
    match have {
        0 => State::Absent,
        all if all == files.len() => State::Applied,
        _ => State::Mixed,
    }
}

pub fn states_for<S: AsRef<str>>(files: &[Vec<S>]) -> [State; 7] {
    NAMES.map(|name| state_for(files, name))
}

// The same states from a backend read: the names every target carries, then the names only some carry.
pub fn states_from<S: AsRef<str>>(applied: &[S], mixed: &[S]) -> [State; 7] {
    let holds = |list: &[S], name: &str| list.iter().any(|tag| tag.as_ref() == name);
    NAMES.map(|name| {
        if holds(applied, name) {
            State::Applied
        } else if holds(mixed, name) {
            State::Mixed
        } else {
            State::Absent
        }
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Add,
    Remove,
    Clear,
}

impl Op {
    // Sample input: "remove"; any other op word the backend sends reads as an add, as ui/js/Tags.js changedLine does.
    pub fn from_wire(word: &str) -> Self {
        match word {
            "clear" => Self::Clear,
            "remove" => Self::Remove,
            _ => Self::Add,
        }
    }
}

// The status sentence for a settagged answer, read from the op the backend applied; name is the tag's shown name.
pub fn changed_line(catalog: &I18n, op: Op, ok: i64, name: &str) -> String {
    match op {
        Op::Clear => catalog.trn("places.tag.cleared", ok),
        Op::Remove => catalog.trn_args("places.tag.removed", ok, &[name]),
        Op::Add => catalog.trn_args("places.tag.tagged", ok, &[name]),
    }
}

// The skip fragment: the backend leaves out every row that is neither a file nor a folder, so it counts items.
pub fn skipped_line(catalog: &I18n, skipped: usize) -> String {
    catalog.trn("places.tag.skipped", skipped as i64)
}

// The undo hint the preset's own undo key fills, empty when no key is bound.
pub fn undo_hint(catalog: &I18n, preset: &str) -> String {
    presentation::hinted(catalog, "places.tag.undo_hint", preset, "undo")
}

// The whole answer line: the change, the skip ahead of a z undo hint, only the skip when nothing changed, empty when neither holds.
pub fn done_line(
    catalog: &I18n,
    preset: &str,
    op: Op,
    ok: i64,
    name: &str,
    skipped: usize,
) -> String {
    done_line_hinted(catalog, &undo_hint(catalog, preset), op, ok, name, skipped)
}

// done_line over the undo hint the preset resolved, empty when no undo key is bound.
pub fn done_line_hinted(
    catalog: &I18n,
    hint: &str,
    op: Op,
    ok: i64,
    name: &str,
    skipped: usize,
) -> String {
    let skip = if skipped > 0 {
        skipped_line(catalog, skipped)
    } else {
        String::new()
    };
    if ok <= 0 {
        return skip;
    }
    let changed = changed_line(catalog, op, ok, name);
    // As v0.3.12 Tags.js doneLine, the skip rides only on a trailing " · z undoes"; another undo key, or none, shows no skip.
    if skip.is_empty() || hint != catalog.tr_args("places.tag.undo_hint", &[&catalog.keycap("z")]) {
        return changed + hint;
    }
    catalog.tr_args("places.tag.with_skip", &[&changed, &skip]) + hint
}

// One flyout tag row; the swatch draws from the theme's menu colours or, in the fallback, the framed digit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagRow {
    pub name: &'static str,
    pub label_key: &'static str,
    pub digit: &'static str,
    pub state: State,
    pub fallback: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlyoutRow {
    Tag(TagRow),
    Separator,
    // Remove All Tags, present only while some target carries a tag (0.4.0 tags-menu board).
    Clear,
}

pub const CLEAR_LABEL_KEY: &str = "places.tag.remove_all";

// The seven tags with their digit hints, then Remove All Tags under a separator while a tag is in effect.
pub fn flyout(states: &[State; 7], fallback: bool) -> Vec<FlyoutRow> {
    let mut rows: Vec<FlyoutRow> = NAMES
        .iter()
        .zip(states)
        .map(|(name, state)| {
            FlyoutRow::Tag(TagRow {
                name,
                label_key: label_key(name),
                digit: key_for(name),
                state: *state,
                fallback,
            })
        })
        .collect();
    if states.iter().any(|state| *state != State::Absent) {
        rows.extend([FlyoutRow::Separator, FlyoutRow::Clear]);
    }
    rows
}

// One rows reply's verdict: whether the folder's slot stands, and whether the in-use set grew.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamped {
    pub latched: bool,
    pub grew: bool,
}

// Every row of a reply learns whether the tag slot stands; in_use gains the tags seen and fresh starts the latch over.
pub fn stamp<S: AsRef<str>>(
    rows: &[Option<Vec<S>>],
    latched: bool,
    in_use: &mut [bool; 7],
    fresh: bool,
) -> Stamped {
    let mut tagged = false;
    let mut grew = false;
    for names in rows.iter().flatten() {
        tagged = true;
        for at in names.iter().filter_map(|name| index_of(name.as_ref())) {
            grew |= !in_use[at];
            in_use[at] = true;
        }
    }
    Stamped {
        latched: tagged || (latched && !fresh),
        grew,
    }
}

// The TAGS section's rows: only the tags in use, in menu order.
pub fn rail_tags(in_use: &[bool; 7]) -> impl Iterator<Item = &'static str> + '_ {
    NAMES
        .iter()
        .zip(in_use)
        .filter(|(_, used)| **used)
        .map(|(name, _)| *name)
}

// How many whole swatches fit the room a row reserves (ROW_SLOT) or a tile's caption gives, with no counter.
pub fn swatches_that_fit(count: usize, room: f32) -> usize {
    let fit = ((room + SWATCH_GAP) / (SWATCH_SIZE + SWATCH_GAP))
        .floor()
        .max(0.0) as usize;
    count.min(fit)
}
