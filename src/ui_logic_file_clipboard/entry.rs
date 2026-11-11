//! A clipboard entry owns its eventual token until its last selection, pending set or paste capture is dropped.
use std::sync::{Arc, OnceLock};

/// Shared identity for one clipboard replacement, independent of asynchronous read generations.
#[derive(Clone, Debug, Default)]
pub struct EntryHandle(Arc<OnceLock<String>>);

impl EntryHandle {
    pub(super) fn new(token: &str) -> Self {
        let entry = Self::default();
        entry.stamp(token);
        entry
    }

    pub(super) fn stamp(&self, token: &str) {
        if !token.is_empty() {
            let _ = self.0.set(token.to_string());
        }
    }

    pub(super) fn token(&self) -> &str {
        self.0.get().map(String::as_str).unwrap_or("")
    }

    #[cfg(test)]
    pub(super) fn weak(&self) -> std::sync::Weak<OnceLock<String>> {
        Arc::downgrade(&self.0)
    }
}

impl PartialEq for EntryHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for EntryHandle {}
