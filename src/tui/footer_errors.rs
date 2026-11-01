// The footer's error row: one error shown for the error time, the rest queued behind it.
use super::model::Model;
use std::time::Instant;

// An error or an undo receipt takes twice the message time to read, then clears on its own (GM 2026-10-20).
pub(super) const ERROR_TIME: std::time::Duration = std::time::Duration::from_secs(8);

impl Model {
    // A repeat of the shown error restarts its time, so a retry that fails again is read in full.
    pub fn fail(&mut self, error: String) {
        if let Some(slot) = self.enqueue_error(error) {
            self.words.error(slot, None);
        }
    }
    pub(super) fn enqueue_error(&mut self, error: String) -> Option<super::words::ErrorSlot> {
        self.words.sync_errors(self.errors.len());
        if error.is_empty() {
            return None;
        }
        if self.error.is_empty() || error == self.error {
            self.error = error;
            self.error_at = Instant::now();
            Some(super::words::ErrorSlot::Current)
        } else if !self.errors.contains(&error) {
            self.errors.push_back(error);
            Some(super::words::ErrorSlot::Queued)
        } else {
            None
        }
    }
    pub fn fail_preview(&mut self, text: super::words::Text) {
        self.remember_preview_error(text.render(&self.i18n), false);
        self.fail_text(text);
    }
    // A reload of the same file keeps its page, so the page is not asked again and its error stays until the file changes.
    pub fn fail_page(&mut self, text: super::words::Text) {
        self.remember_preview_error(text.render(&self.i18n), true);
        self.fail_text(text);
    }
    fn remember_preview_error(&mut self, error: String, page: bool) {
        if !error.is_empty()
            && !self
                .preview_errors
                .iter()
                .any(|(posted, _)| *posted == error)
        {
            self.preview_errors.push((error, page));
        }
    }
    // A preview's failure leaves the footer once its file's preview is no longer drawn, or once a reload asks the failed part again.
    pub fn retract_preview_errors(&mut self, file_changed: bool) {
        let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.preview_errors)
            .into_iter()
            .partition(|(_, page)| file_changed || !page);
        self.preview_errors = kept;
        self.words.retract_errors(&mut self.errors, &gone);
        if gone.iter().any(|(posted, _)| *posted == self.error) {
            self.dismiss_error();
        }
    }
    pub fn dismiss_error(&mut self) {
        self.words.sync_errors(self.errors.len());
        self.error = self.errors.pop_front().unwrap_or_default();
        self.words.dismiss_error();
        self.error_at = Instant::now();
    }
    pub(super) fn expire_error(&mut self, now: Instant) {
        if !self.error.is_empty() && now.saturating_duration_since(self.error_at) >= ERROR_TIME {
            self.dismiss_error();
            self.error_at = now;
        }
    }
}

#[cfg(test)]
#[path = "footer_errors_tests.rs"]
mod tests;
