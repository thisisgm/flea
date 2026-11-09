//! The flash pane's decisions: the chosen drive held by identity, the poll and the endings' figures.
use super::flash::{fits, preselect, Drive};
use super::flashcard;

// A drive is the disk the backend re-checks at flash time, so the pane holds its choice by identity.
pub fn same(a: &Drive, b: &Drive) -> bool {
    a.dev == b.dev && a.seq == b.seq && a.serial == b.serial
}

pub fn index_of(drives: &[Drive], drive: &Drive) -> Option<usize> {
    drives.iter().position(|row| same(row, drive))
}

/// What a cursor position names: the image, a drive by identity, or the Flash step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Image,
    Drive(usize),
    Flash,
}

// Sample input: cursor 1 names drive 0; cursor past the drives names Flash.
pub fn target(cursor: usize, drives: &[Drive]) -> Target {
    if cursor == 0 {
        return Target::Image;
    }
    if cursor > drives.len() {
        return Target::Flash;
    }
    Target::Drive(cursor - 1)
}

// Sample input: moving onto a refused drive chooses nothing; onto a usable one chooses it.
pub fn drive_at(cursor: usize, drives: &[Drive]) -> Option<Drive> {
    match target(cursor, drives) {
        Target::Drive(at) if fits(&drives[at]) => Some(drives[at].clone()),
        _ => None,
    }
}

// Sample input: a move landing on Flash keeps the kept choice.
pub fn choose_kept(cursor: usize, drives: &[Drive], kept: Option<Drive>) -> Option<Drive> {
    drive_at(cursor, drives).or(kept)
}

/// One drive-list answer folded in: the choice kept by identity, the cursor kept on what it named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fold {
    pub chosen: Option<Drive>,
    pub held: bool,
    pub selected: Option<usize>,
    pub cursor: usize,
    pub moved: bool,
}

/// The poll's input: the held choice, the cursor, the last list and the rail's pending disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Poll {
    pub chosen: Option<Drive>,
    pub held: bool,
    pub cursor: usize,
    pub drives: Vec<Drive>,
    pub first: bool,
    pub pending_disk: Option<String>,
}

// Sample input: a stick plugged in ahead of the chosen drive leaves it chosen.
pub fn reconcile(state: &Poll, rows: &[Drive]) -> Fold {
    let list = rows.to_vec();
    let held = !state.first && (state.held || state.chosen.is_some());
    let mut chosen = None;
    if let Some(pending) = &state.pending_disk {
        let disk = super::flash::rail_disk(pending);
        chosen = list.iter().find(|row| row.dev == disk && fits(row)).cloned();
    } else {
        if let Some(keep) = &state.chosen {
            if let Some(at) = index_of(&list, keep) {
                if fits(&list[at]) {
                    chosen = Some(list[at].clone());
                }
            }
        }
        if chosen.is_none() && !held {
            if let Some(at) = preselect(&list) {
                chosen = Some(list[at].clone());
            }
        }
    }
    let selected = chosen.as_ref().and_then(|drive| index_of(&list, drive));
    let held = held || chosen.is_some();
    if state.first || state.pending_disk.is_some() {
        return Fold {
            cursor: selected.map_or(0, |at| at + 1),
            chosen,
            held,
            selected,
            moved: true,
        };
    }
    let was = target(state.cursor, &state.drives);
    let cursor = match &was {
        Target::Image => 0,
        Target::Flash => super::flash::flash_pos(list.len()),
        Target::Drive(_) => {
            let named = match &was {
                Target::Drive(at) => state.drives.get(*at).and_then(|drive| index_of(&list, drive)),
                _ => None,
            };
            match named {
                Some(at) if fits(&list[at]) => at + 1,
                _ => selected.map_or(0, |at| at + 1),
            }
        }
    };
    let now = target(cursor, &list);
    let moved = match (&now, &was) {
        (Target::Drive(at), Target::Drive(was_at)) => match state.drives.get(*was_at) {
            None => true,
            Some(before) => !same(&list[*at], before),
        },
        _ => now != was,
    };
    Fold { chosen, held, selected, cursor, moved }
}

/// The view's marks an answer lands on: the rail's pending disk is spent with it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewMark {
    pub pending_drive: String,
    pub chosen: Option<Drive>,
    pub held: bool,
    pub cursor: usize,
}

// Sample input: an answer lands its choice, mark and cursor and spends the rail's disk.
pub fn land(view: &mut ViewMark, next: &Fold) {
    view.pending_drive.clear();
    view.chosen.clone_from(&next.chosen);
    view.held = next.held;
    view.cursor = next.cursor;
}

/// The host's pane mark follows the pane it covers, as an index into the window's panes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Host {
    pub cover_pane: Option<usize>,
}

// Sample input: opening on the other pane clears the first pane's mark.
pub fn cover(host: &mut Host, panes: &mut [bool], pane: usize) {
    if let Some(covered) = host.cover_pane {
        if covered != pane {
            panes[covered] = false;
        }
    }
    host.cover_pane = Some(pane);
    panes[pane] = true;
}

// Sample input: leaving through the view's own Back clears the pane's mark.
pub fn uncover(host: &mut Host, panes: &mut [bool]) {
    if let Some(covered) = host.cover_pane {
        panes[covered] = false;
    }
}

/// The transfer Back handed the pane: reopening onto a running flash takes it back; a copy's stays.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Handed {
    pub id: u64,
    pub flash: bool,
    pub running: bool,
}

// Sample input: the reopened pane no longer draws the flash's card nor its sticky line.
pub fn reclaim(transfer: &mut Handed, sticky: &mut String) {
    if !transfer.flash {
        return;
    }
    *transfer = Handed::default();
    sticky.clear();
}

/// Which press a flashdone answers: the running flash, a refusal of the press just sent, or neither.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DoneFor {
    Finished,
    Refused,
    Ignore,
}

// Sample input: a probe refusal for the press just sent is that press's answer.
pub fn done_for(running: bool, transfer_id: u64, press_sent: bool, pending_id: u64, id: u64) -> DoneFor {
    if running {
        return if id == transfer_id { DoneFor::Finished } else { DoneFor::Ignore };
    }
    if press_sent && id == pending_id { DoneFor::Refused } else { DoneFor::Ignore }
}

/// The ending's figures: a verify miss names the backend's offset, every other ending the bytes the drive took.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Figures {
    pub at: u64,
    pub total: u64,
}

// Sample input: a cancel reports the bytes the drive took, the backend's at being 0 there.
pub fn ending_context(transfer: &flashcard::Transfer, err: &str, at: u64) -> Figures {
    Figures {
        at: if err == "verify" { at } else { transfer.bytes },
        total: flashcard::total(transfer),
    }
}
