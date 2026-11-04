//! The Trash view's marks: an ordered uri map (true marked, false excluded from a bulk selection) with reviewed identities.
use super::rows::TrashRow;

/// How a click or key extends the marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extend {
    No,
    Toggle,
    Add,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    // Insertion order, as the oracle's object keys keep it, so requests name uris in the order they were marked.
    marks: Vec<(String, bool)>,
    identities: Vec<(String, String)>,
    /// A backend select-all is live: rows it covers read their bulkSelected flag.
    pub all: bool,
    pub token: u64,
    pub count: usize,
}

fn put<T: Clone>(list: &mut Vec<(String, T)>, key: &str, value: T) {
    match list.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value,
        None => list.push((key.into(), value)),
    }
}

impl Selection {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
    pub fn clear_bulk(&mut self) {
        self.all = false;
        self.token = 0;
        self.count = 0;
    }
    pub fn mark(&self, uri: &str) -> Option<bool> {
        self.marks.iter().find(|(k, _)| k == uri).map(|(_, v)| *v)
    }
    pub fn keys(&self) -> Vec<String> {
        self.marks.iter().map(|(k, _)| k.clone()).collect()
    }
    pub fn uris(&self) -> Vec<String> {
        self.marks.iter().filter(|(_, v)| *v).map(|(k, _)| k.clone()).collect()
    }
    pub fn excluded(&self) -> Vec<String> {
        self.marks.iter().filter(|(_, v)| !*v).map(|(k, _)| k.clone()).collect()
    }
    pub fn identity(&self, uri: &str) -> String {
        self.identities.iter().find(|(k, _)| k == uri).map(|(_, v)| v.clone()).unwrap_or_default()
    }
    pub fn identities_of(&self, uris: &[String]) -> Vec<String> {
        uris.iter().map(|uri| self.identity(uri)).collect()
    }

    /// Marked rows, counting a live bulk selection less its exclusions.
    pub fn len(&self) -> usize {
        let uris = self.uris().len();
        if self.all {
            self.count.saturating_sub(self.excluded().len()) + uris
        } else {
            uris
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// An explicit mark decides; otherwise a bulk selection reads the held row's flag.
    pub fn is_selected(&self, uri: &str, held: &[TrashRow]) -> bool {
        if let Some(mark) = self.mark(uri) {
            return mark;
        }
        self.all && held.iter().any(|row| row.uri == uri && row.bulk_selected)
    }

    /// TrashView.choose's marking half for a held row.
    pub fn choose(&mut self, row: &TrashRow, extend: Extend, held: &[TrashRow]) {
        let mut next = if extend == Extend::No { Self::default() } else { self.clone() };
        let value = match extend {
            Extend::Toggle => !self.is_selected(&row.uri, held),
            _ => true,
        };
        put(&mut next.marks, &row.uri, value);
        if value {
            put(&mut next.identities, &row.uri, row.identity.clone());
        } else {
            next.identities.retain(|(k, _)| *k != row.uri);
        }
        if self.all && extend != Extend::No && value && row.bulk_selected {
            next.marks.retain(|(k, _)| *k != row.uri);
            next.identities.retain(|(k, _)| *k != row.uri);
        }
        if extend == Extend::No {
            next.clear_bulk();
        }
        *self = next;
    }

    /// A list reply keeps only marks the backend still found.
    pub fn keep_present(&mut self, present: &[String]) {
        self.marks.retain(|(k, _)| present.contains(k));
        self.identities.retain(|(k, _)| present.contains(k));
    }

    /// After a restore or delete only the failed items stay marked, with the identity each failure reported.
    pub fn failures(&mut self, failed: &[(String, String)]) {
        self.clear();
        for (uri, identity) in failed {
            put(&mut self.marks, uri, true);
            if !identity.is_empty() {
                put(&mut self.identities, uri, identity.clone());
            }
        }
    }
}
