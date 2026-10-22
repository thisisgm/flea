//! The muted lane's parts: per-owner activities, the joined secondary text and the selection's byte total.
use super::status::SEP;

/// One owner's activity; ids are meaningful only with their owner, because each backend numbers its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    pub owner: u64,
    pub text: String,
    pub transfer: u64,
    pub running: bool,
    pub starting: bool,
    pub cancelling: bool,
}

/// Sets, replaces or (with empty text) clears one owner's activity; a cancel survives progress on the same id.
pub fn activity_changed(
    activities: &mut Vec<Activity>,
    owner: u64,
    text: &str,
    transfer: u64,
    running: bool,
    starting: bool,
) {
    let at = activities
        .iter()
        .position(|activity| activity.owner == owner);
    if text.is_empty() {
        if let Some(at) = at {
            activities.remove(at);
        }
        return;
    }
    let cancelling =
        at.is_some_and(|at| activities[at].transfer == transfer && activities[at].cancelling);
    let activity = Activity {
        owner,
        text: text.into(),
        transfer,
        running,
        starting,
        cancelling,
    };
    match at {
        Some(at) => activities[at] = activity,
        None => activities.push(activity),
    }
}

/// Marks the displayed transfer cancelling; false when there was nothing to change.
pub fn cancel_activity(activities: &mut [Activity]) -> bool {
    match activities.first_mut() {
        Some(first) if first.running && !first.cancelling => {
            first.cancelling = true;
            true
        }
        _ => false,
    }
}

/// Only the displayed activity's Starting line walks the busy mark, never one queued behind it.
pub fn displayed_starting(activities: &[Activity]) -> bool {
    activities.first().is_some_and(|activity| activity.starting)
}

/// The muted lane as one string: hint, a sticky under an error, the queued activities, a search, a retry.
#[allow(clippy::too_many_arguments)]
pub fn secondary_text(
    key_hint: &str,
    transient_is_error: bool,
    sticky_here: bool,
    sticky: &str,
    activities: &[Activity],
    searching: bool,
    search_line: &str,
    retry_line: &str,
) -> String {
    let extra = activities
        .iter()
        .skip(1)
        .map(|activity| activity.text.as_str())
        .collect::<Vec<_>>()
        .join(SEP);
    [
        key_hint,
        if transient_is_error && sticky_here {
            sticky
        } else {
            ""
        },
        &extra,
        if sticky_here && !transient_is_error && searching {
            search_line
        } else {
            ""
        },
        if transient_is_error { "" } else { retry_line },
    ]
    .iter()
    .filter(|part| !part.is_empty())
    .map(|part| format!("{SEP}{part}"))
    .collect()
}

/// A held row's size facts; a folder's bytes come only from a finished walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowSize {
    pub dir: bool,
    pub bytes: u64,
}

/// A folder walk's answer: its bytes and whether it stopped early.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Walked {
    pub bytes: u64,
    pub partial: bool,
}

/// The selection's byte total, or None when any selected row is not held or any size is unknown; nothing here starts a walk.
pub fn selection_bytes(
    selected: &[usize],
    held_start: usize,
    rows: &[RowSize],
    walked: impl Fn(usize) -> Option<Walked>,
) -> Option<u64> {
    if selected.is_empty() || selected.len() > rows.len() {
        return None;
    }
    let mut bytes = 0u64;
    for &index in selected {
        let row = rows.get(index.checked_sub(held_start)?)?;
        if row.dir {
            match walked(index) {
                Some(walk) if !walk.partial => bytes += walk.bytes,
                _ => return None,
            }
        } else {
            bytes += row.bytes;
        }
    }
    Some(bytes)
}
