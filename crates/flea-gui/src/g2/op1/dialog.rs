//! What each open dialog answers about itself: its card kind, its text field and the session typing goes to.
use super::model::{CopyFocus, Dialog};
use flea::ui_logic_op1::card::Kind;

impl Dialog {
    pub fn kind(&self) -> Kind {
        match self {
            Dialog::Collide { .. } | Dialog::Fat { .. } => Kind::Collide,
            Dialog::Delete { .. } => Kind::Delete,
            Dialog::CopyTo { .. } | Dialog::SendTo { .. } => Kind::CopyTo,
            Dialog::NewFile { .. } => Kind::NewFile,
        }
    }

    // The session and draft of the dialog's text field, for each dialog with one.
    pub(super) fn draft(&self) -> Option<(u64, &str)> {
        match self {
            Dialog::CopyTo { menu, path, .. } | Dialog::SendTo { menu, path, .. } => {
                Some((*menu, path))
            }
            Dialog::NewFile { id, name, .. } => Some((*id, name)),
            _ => None,
        }
    }

    pub(super) fn draft_mut(&mut self) -> Option<&mut String> {
        match self {
            Dialog::CopyTo { path, .. } | Dialog::SendTo { path, .. } => Some(path),
            Dialog::NewFile { name, .. } => Some(name),
            _ => None,
        }
    }

    // The focus of the dialog's field and its busy flag, for each dialog with a field.
    pub(super) fn field_state(&mut self) -> Option<(&mut CopyFocus, bool)> {
        match self {
            Dialog::CopyTo { focus, busy, .. }
            | Dialog::SendTo { focus, busy, .. }
            | Dialog::NewFile { focus, busy, .. } => Some((focus, *busy)),
            _ => None,
        }
    }

    // The session whose field takes typing now: focused on the field and not busy.
    pub(super) fn typing_session(&self) -> Option<u64> {
        match self {
            Dialog::CopyTo {
                menu,
                focus: CopyFocus::Field,
                busy: false,
                ..
            }
            | Dialog::SendTo {
                menu,
                focus: CopyFocus::Field,
                busy: false,
                ..
            } => Some(*menu),
            Dialog::NewFile {
                id,
                focus: CopyFocus::Field,
                busy: false,
                ..
            } => Some(*id),
            _ => None,
        }
    }
}
