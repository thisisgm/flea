//! Contributions extend the existing archive and sharing groups without regrouping native rows.
use flea::ui_logic_mn1::{inventory, Entry};

fn group(row: &Entry) -> &str {
    inventory::spec_of(&row.id).map_or("", |spec| spec.group)
}

fn boundary(entries: &[Entry], groups: &[&str]) -> usize {
    entries
        .iter()
        .position(|row| groups.contains(&group(row)))
        .map(|at| {
            if at > 0 && entries[at - 1].separator {
                at - 1
            } else {
                at
            }
        })
        .unwrap_or(entries.len())
}

fn insert_group(entries: &mut Vec<Entry>, at: usize, mut rows: Vec<Entry>, new_group: bool) {
    if rows.is_empty() {
        return;
    }
    if new_group && at > 0 && !entries[at - 1].separator {
        rows.insert(0, Entry::separator());
    }
    if at < entries.len() && !entries[at].separator {
        rows.push(Entry::separator());
    }
    entries.splice(at..at, rows);
}

pub(crate) fn insert_actions(entries: &mut Vec<Entry>, rows: Vec<Entry>) {
    if let Some(at) = entries.iter().rposition(|row| group(row) == "archive") {
        insert_group(entries, at + 1, rows, false);
    } else {
        let at = boundary(entries, &["share", "inspect", "view", "settings"]);
        insert_group(entries, at, rows, true);
    }
}

pub(super) fn insert_sends(entries: &mut Vec<Entry>, rows: Vec<Entry>) {
    if let Some(at) = entries.iter().rposition(|row| group(row) == "share") {
        insert_group(entries, at + 1, rows, false);
    } else {
        let at = boundary(entries, &["inspect", "view", "settings"]);
        insert_group(entries, at, rows, true);
    }
}
