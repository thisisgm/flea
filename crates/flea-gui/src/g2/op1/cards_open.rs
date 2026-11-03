//! Existing delete and copy card entry points on Operations.
use super::*;

impl Operations {
    /// Present another producer's review after ending or holding the previous card's session.
    pub fn present_external_delete(&mut self, review: Review, scope: Scope, token: u64) {
        self.clear_card();
        self.external_delete = Some(token);
        self.review_session += 1;
        self.dialog = Some(Dialog::Delete {
            session: self.review_session,
            review,
            scope,
            focus: trash::PRIMARY,
            busy: false,
        });
    }

    // The permanent card, for the menu's Delete permanently or the Trash view's own delete, from its prepare reply.
    pub fn confirm_delete(&mut self, review: Review, scope: Scope, source: Source, now: u64) {
        self.clear_card();
        self.review_session += 1;
        self.confirm = Some(Confirm::new(source, scope, review.clone(), now));
        self.dialog = Some(Dialog::Delete {
            session: self.review_session,
            review,
            scope,
            focus: trash::PRIMARY,
            busy: false,
        });
    }

    // Copy To or Move To from the menu row with snapshot id `menu`.
    pub fn copy_to(&mut self, menu: u64, moving: bool, path: &str) {
        self.clear_card();
        self.dialog = Some(Dialog::CopyTo {
            menu,
            moving,
            path: path.into(),
            focus: CopyFocus::Field,
            busy: false,
            error: String::new(),
        });
    }

    // The folder's New File card: nothing is sent until Create, and the name opens whole for typing, as v0.3.12 selects it.
}
