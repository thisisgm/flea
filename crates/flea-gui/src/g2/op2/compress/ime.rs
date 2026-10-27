//! Archive fields expose their drafts to the shared input adapter.
use super::*;

impl Compress {
    pub(crate) fn ime_revealed(&self) -> bool {
        match self.focus {
            Stop::Password => self.held_password,
            Stop::Confirm => self.held_confirm,
            _ => false,
        }
    }
    pub(crate) fn ime_value(&self) -> Option<(crate::g2::op1::dialog_editor::Slot, &str)> {
        use crate::g2::op1::dialog_editor::Slot;
        match self.focus {
            Stop::Name => Some((Slot::ArchiveName, &self.name)),
            Stop::Password => Some((Slot::ArchivePassword, &self.password)),
            Stop::Confirm => Some((Slot::ArchiveConfirm, &self.confirm)),
            _ => None,
        }
    }
    pub(crate) fn ime_set(&mut self, slot: crate::g2::op1::dialog_editor::Slot, text: &str) {
        use crate::g2::op1::dialog_editor::Slot;
        match slot {
            Slot::ArchiveName => self.name = text.into(),
            Slot::ArchivePassword => self.password = text.into(),
            Slot::ArchiveConfirm => self.confirm = text.into(),
            _ => return,
        }
        self.error = None;
    }
}
