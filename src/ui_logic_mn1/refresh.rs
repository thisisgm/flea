//! What a provider refresh does to an open menu, and whether a choice still names a live row.
use super::build::Peer;
use super::entry::Entry;

/// The provider inputs at this moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Live {
    pub refreshing: bool,
    pub taildrop_installed: bool,
    pub taildrop_peers: Vec<Peer>,
    pub dropbox_installed: bool,
    pub dropbox_path: String,
    pub localsend_checking: bool,
    pub localsend_answered: bool,
}

/// A finished refresh's answer, which later opens draw while their own refresh runs behind them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settled {
    pub taildrop_installed: bool,
    pub taildrop_peers: Vec<Peer>,
    pub dropbox_installed: bool,
    pub dropbox_path: String,
}

pub fn settle(answer: &Live) -> Settled {
    Settled {
        taildrop_installed: answer.taildrop_installed,
        taildrop_peers: answer.taildrop_peers.clone(),
        dropbox_installed: answer.dropbox_installed,
        dropbox_path: answer.dropbox_path.clone(),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub taildrop_peers: Vec<Peer>,
    pub taildrop_refreshing: bool,
    pub dropbox_path: String,
    pub dropbox_refreshing: bool,
    pub localsend_checking: bool,
}

// A kept answer for the same install state stands in while a refresh runs.
pub fn provider_view(known: Option<&Settled>, now: &Live) -> View {
    let taildrop =
        known.filter(|k| now.refreshing && k.taildrop_installed == now.taildrop_installed);
    let dropbox = known.filter(|k| now.refreshing && k.dropbox_installed == now.dropbox_installed);
    View {
        taildrop_peers: taildrop
            .map_or_else(|| now.taildrop_peers.clone(), |k| k.taildrop_peers.clone()),
        taildrop_refreshing: now.refreshing && taildrop.is_none(),
        dropbox_path: dropbox.map_or_else(|| now.dropbox_path.clone(), |k| k.dropbox_path.clone()),
        dropbox_refreshing: now.refreshing && dropbox.is_none(),
        localsend_checking: now.localsend_checking && !now.localsend_answered,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub cursor: usize,
    // The open flyout's parent row, None when the flyout closes.
    pub submenu_row: Option<usize>,
    pub submenu_cursor: usize,
}

// Action and peer identity survive a rebuild beneath the keyboard cursor.
pub fn refreshed_cursor(
    previous: &[Entry],
    next: &[Entry],
    cursor: usize,
    submenu_row: Option<usize>,
    submenu_cursor: usize,
) -> Cursor {
    let action = previous
        .get(cursor)
        .map(|entry| entry.action.as_ref())
        .filter(|a| !a.is_empty());
    let found = action.and_then(|action| next.iter().position(|entry| entry.action == action));
    let selected = found.unwrap_or_else(|| {
        let len = next.len() as isize;
        let mut at = (cursor as isize).min(len - 1).max(0);
        while at < len && at >= 0 && !next[at as usize].steppable() {
            at += 1;
        }
        if at == len {
            at -= 1;
            while at >= 0 && !next[at as usize].steppable() {
                at -= 1;
            }
        }
        at.max(0) as usize
    });
    let old = submenu_row.and_then(|row| previous.get(row));
    let sub_row = old.and_then(|old| next.iter().position(|entry| entry.action == old.action));
    let old_target = old
        .and_then(|old| old.submenu.as_ref())
        .and_then(|leaves| leaves.get(submenu_cursor));
    let target = match (sub_row.map(|row| &next[row]), old_target) {
        (Some(sub), Some(old_target)) if !sub.disabled => sub
            .submenu
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .position(|leaf| leaf.id == old_target.id && !leaf.disabled),
        _ => None,
    };
    // A flyout that held only a waiting row (AirDrop's Looking for devices…) stays open on the leaves that replace it.
    let waiting = old
        .and_then(|old| old.submenu.as_deref())
        .is_some_and(|leaves| leaves.iter().all(|leaf| leaf.disabled));
    let target = target.or_else(|| {
        let sub = sub_row
            .map(|row| &next[row])
            .filter(|sub| waiting && !sub.disabled)?;
        let leaves = sub.submenu.as_deref().filter(|leaves| !leaves.is_empty())?;
        Some(leaves.iter().position(|leaf| !leaf.disabled).unwrap_or(0))
    });
    Cursor {
        cursor: selected,
        submenu_row: target.and(sub_row),
        submenu_cursor: target.unwrap_or(0),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    // The selection the menu opened over has changed.
    Moved,
    Unavailable,
}

impl Refusal {
    pub fn key(self) -> &'static str {
        match self {
            Self::Moved => "menu.refusal.moved",
            Self::Unavailable => "menu.refusal.unavailable",
        }
    }
}

pub const EMPTY_CLIPBOARD: &str = "menu.refusal.empty_clipboard";

// A choice fires only if the rebuilt rows still offer it live, under the selection the menu opened over.
pub fn validate_choice(
    live: &[Entry],
    action: &str,
    sub_id: &str,
    identity_changed: bool,
) -> Result<(), Refusal> {
    if identity_changed {
        return Err(Refusal::Moved);
    }
    let offered = live
        .iter()
        .filter(|entry| entry.action == action && !entry.disabled)
        .any(|entry| {
            sub_id.is_empty()
                || entry
                    .submenu
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .any(|leaf| leaf.id == sub_id && !leaf.disabled)
        });
    if offered {
        Ok(())
    } else {
        Err(Refusal::Unavailable)
    }
}
