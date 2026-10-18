// Job Undo tokens and the busy-lock retry queue, beside the journal they guard.
use super::{Entry, Journal, UndoToken};
use crate::error::FleaError;

impl Journal {
    // Only an entry filed now can offer job Undo; a held reservation or a running publication publishes no premature token.
    pub(crate) fn push_for_job(&mut self, entry: Entry) -> Option<UndoToken> {
        if let Some(queue) = self.deferred.as_mut() {
            queue.push(crate::backend::renamework::Publication::Push(entry));
            return None;
        }
        let entry = self.gate.hold(entry)?;
        self.file_inner(entry, true);
        self.recorded.take()
    }

    pub(crate) fn undo_for_job(&mut self, token: &UndoToken) -> Result<String, FleaError> {
        self.undo_matching(Some(token))
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    pub(crate) fn retry_pending(&mut self) {
        let mut pending = std::mem::take(&mut self.pending).into_iter();
        while let Some(entry) = pending.next() {
            self.file(entry);
            if self.has_pending() {
                self.pending.extend(pending);
                break;
            }
        }
    }

    pub(crate) fn token_is_current(&self, key: &UndoToken) -> bool {
        key.revision == self.revision && key.generation.is_some() == self.shared.is_some()
    }
}
