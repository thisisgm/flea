//! The unlock card's eye: the typed secret shows only while the eye is held, as the archive and share dialogs' eyes do.
use super::unlock::{Card, Phase};

impl Card {
    pub fn eye_press(&mut self) {
        if self.opened && self.phase != Phase::Unlocking {
            self.field.reveal();
        }
    }

    pub fn eye_release(&mut self) {
        self.field.conceal();
    }

    pub fn revealed(&self) -> bool {
        self.field.revealed
    }
}
