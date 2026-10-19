// The Permissions card's mode arithmetic, the several-item grid and the notes that name what keeps its mode.
use crate::i18n::I18n;

/// Keeps the one-shot Make executable id clear of the card's own batch ids.
pub const MAKE_EXEC_ID: u64 = 1_000_000;
/// Inspect ids are request blocks of this width; a wider selection reserves whole blocks.
pub const INSPECT_STRIDE: u64 = 1000;
/// The nine rwx bits of a mode, below the special bits.
pub const PERMISSION_BITS: u32 = 0o777;
/// Most names a skip line lists before it counts the rest.
pub const SKIP_NAMES_SHOWN: usize = 3;
/// The backend's own sentence for a file another user owns, matched as machine identity.
pub const NOT_OWNER: &str = "Read-only: you are not the owner.";
/// The backend's fallback refusal when a reply names none.
pub const FAILED: &str = "Could not change permissions.";

/// Only the newest id on the asked path lands, so a late answer never arms a later file.
pub fn lands_shebang(path: &str, id: u64, asked: &str, current: u64) -> bool {
    path == asked && id == current
}

fn octal_digits(text: &str, lengths: &[usize]) -> bool {
    lengths.contains(&text.len()) && text.bytes().all(|b| (b'0'..=b'7').contains(&b))
}

/// Sample input: "644" or "0644"; any other text, special bits included, answers None.
pub fn parse(text: &str) -> Option<u32> {
    let digits = match text.len() {
        4 if text.starts_with('0') => &text[1..],
        3 => text,
        _ => return None,
    };
    octal_digits(digits, &[3])
        .then(|| u32::from_str_radix(digits, 8).ok())
        .flatten()
}

pub fn octal(value: u32) -> String {
    format!("{value:04o}")
}

/// One box flipped in the octal text; invalid text stays as typed until corrected.
pub fn toggle(text: &str, bit: u32) -> String {
    parse(text).map_or_else(|| text.to_string(), |value| octal(value ^ bit))
}

/// Sample input: "4644" answers 0o644; the nine permission bits of a mode that carries special bits too.
pub fn shown_bits(text: &str) -> Option<u32> {
    octal_digits(text, &[3, 4])
        .then(|| {
            u32::from_str_radix(text, 8)
                .ok()
                .map(|v| v & PERMISSION_BITS)
        })
        .flatten()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bit {
    pub mask: u32,
    pub on: bool,
    pub mixed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub bits: [Bit; 9],
    pub mixed: bool,
    pub changeable: bool,
}

/// A skipped file neither sets nor mixes a bit; when every file is skipped the boxes show their own bits and nothing changes.
pub fn summarize(modes: &[String], reasons: &[String]) -> Summary {
    let mut values: Vec<u32> = Vec::new();
    let mut shown: Vec<u32> = Vec::new();
    for (at, mode) in modes.iter().enumerate() {
        let reasoned = reasons.get(at).is_some_and(|why| !why.is_empty());
        if let (Some(value), false) = (parse(mode), reasoned) {
            values.push(value);
        }
        if let Some(bits) = shown_bits(mode) {
            shown.push(bits);
        }
    }
    let changeable = !values.is_empty();
    if !changeable {
        values = shown;
    }
    let mut summary = Summary {
        changeable,
        ..Summary::default()
    };
    for (b, slot) in summary.bits.iter_mut().enumerate() {
        let mask = 1 << (8 - b);
        let on = values.iter().all(|v| v & mask != 0);
        let off = values.iter().all(|v| v & mask == 0);
        let mixed = !on && !off;
        summary.mixed |= mixed;
        *slot = Bit {
            mask,
            on: !values.is_empty() && on,
            mixed,
        };
    }
    summary
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
    Setuid,
    Setgid,
    Sticky,
}

impl Special {
    pub fn word(self) -> &'static str {
        match self {
            Self::Setuid => "setuid",
            Self::Setgid => "setgid",
            Self::Sticky => "sticky",
        }
    }
}

/// A mode with special bits names the bit the backend would name, never dropped on a failed parse.
pub fn special_of(text: &str) -> Option<Special> {
    let bytes = text.as_bytes();
    if !octal_digits(text, &[4]) || bytes[0] == b'0' {
        return None;
    }
    let special = bytes[0] - b'0';
    Some(if special & 4 != 0 {
        Special::Setuid
    } else if special & 2 != 0 {
        Special::Setgid
    } else {
        Special::Sticky
    })
}

/// The backend's own sentence for a special bit, the identity skip_kind reads back.
pub fn special_reason(text: &str) -> String {
    special_of(text).map_or_else(String::new, |s| {
        format!("Read-only: {} bit is present.", s.word())
    })
}

/// Sample input: "Read-only: setgid bit is present." answers Setgid; any other reason answers None.
pub fn special_label(why: &str) -> Option<Special> {
    let word = why
        .strip_prefix("Read-only: ")?
        .strip_suffix(" bit is present.")?;
    [Special::Setuid, Special::Setgid, Special::Sticky]
        .into_iter()
        .find(|s| s.word() == word)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkipKind {
    Special,
    Owner,
    Other,
}

pub fn skip_kind(why: &str) -> SkipKind {
    if special_label(why).is_some() {
        SkipKind::Special
    } else if why == NOT_OWNER {
        SkipKind::Owner
    } else {
        SkipKind::Other
    }
}

pub fn leaf_of(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, leaf)| leaf)
}

/// A backend reason's own words are never changed, only closed with a full stop.
pub fn closed(text: &str) -> String {
    if text.ends_with('.') {
        text.to_string()
    } else {
        format!("{text}.")
    }
}

/// A refusal the backend names by its fixed sentence, in either case and with or without its stop, maps to the card's own words; any other text stays the backend's.
pub fn refusal(i18n: &I18n, error: &str) -> String {
    const KNOWN: [(&str, &str); 1] = [(
        "an operation is already running",
        "dialogs.file.permissions.busy",
    )];
    let bare = error.trim().trim_end_matches('.');
    match KNOWN
        .iter()
        .find(|(sentence, _)| sentence.eq_ignore_ascii_case(bare))
    {
        Some((_, key)) => i18n.tr(key),
        None if bare.is_empty() => i18n.tr("dialogs.file.permissions.failed"),
        None => closed(error),
    }
}

mod skips;
pub use skips::{multi_result, skip_note, Inspected, Skip, Store};

/// The batch a several-item Apply sends: every changeable file at its target mode, the rest counted as skips.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    pub paths: Vec<String>,
    pub modes: Vec<String>,
    pub skipped: Vec<Skip>,
}

/// An untouched box keeps each file's own bit; set and clear are the boxes the user changed.
pub fn batch(store: &Store, paths: &[String], set: u32, clear: u32) -> Batch {
    let mut out = Batch::default();
    for (at, path) in paths.iter().enumerate() {
        let mode = store.modes.get(at).map_or("", String::as_str);
        let mut why = store.reasons.get(at).cloned().unwrap_or_default();
        let base = parse(mode);
        if why.is_empty() && base.is_none() {
            why = special_reason(mode);
        }
        match (base, why.is_empty()) {
            (Some(base), true) => {
                out.paths.push(path.clone());
                out.modes.push(octal((base & !clear) | set));
            }
            _ => out.skipped.push(Skip {
                path: path.clone(),
                why: if why.is_empty() {
                    FAILED.to_string()
                } else {
                    why
                },
            }),
        }
    }
    out
}

/// The tri-state a box shows over several files once the user's own changes apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxValue {
    On,
    Off,
    Some,
}

pub fn multi_value(summary: &Summary, set: u32, clear: u32, bit: u32) -> BoxValue {
    if set & bit != 0 {
        return BoxValue::On;
    }
    if clear & bit != 0 {
        return BoxValue::Off;
    }
    match summary.bits.iter().find(|b| b.mask == bit) {
        Some(b) if b.mixed => BoxValue::Some,
        Some(b) if b.on => BoxValue::On,
        _ => BoxValue::Off,
    }
}

/// A set box lets go unless mixed, so a uniform-off box reads off, on, off, on.
pub fn multi_toggle(summary: &Summary, set: &mut u32, clear: &mut u32, bit: u32) {
    let found = summary
        .bits
        .iter()
        .find(|b| b.mask == bit)
        .copied()
        .unwrap_or_default();
    if *set & bit != 0 {
        *set &= !bit;
        if found.mixed {
            *clear |= bit;
        }
    } else if *clear & bit != 0 {
        *clear &= !bit;
    } else if found.mixed || !found.on {
        *set |= bit;
        *clear &= !bit;
    } else {
        *clear |= bit;
        *set &= !bit;
    }
}

pub fn inspect_request(id: u64, path: &str) -> String {
    format!(
        r#"{{"c":"permissions","op":"inspect","id":{id},"path":"{}"}}"#,
        crate::json::escape(path)
    )
}

pub fn apply_request(id: u64, mode: &str) -> String {
    format!(
        r#"{{"c":"permissions","op":"apply","id":{id},"mode":"{}"}}"#,
        crate::json::escape(mode)
    )
}

pub fn close_request(id: u64) -> String {
    format!(r#"{{"c":"permissions","op":"close","id":{id}}}"#)
}

pub fn batch_request(id: u64, batch: &Batch) -> String {
    format!(
        r#"{{"c":"permissionsBatch","paths":{},"modes":{},"id":{id}}}"#,
        crate::json::string_array(&batch.paths),
        crate::json::string_array(&batch.modes)
    )
}
