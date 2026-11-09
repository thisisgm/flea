//! Every button a card or a Jobs block draws, named by what its press does.
use flea::ui_logic_file_ops::{
    collide::{Choice, Fat},
    jobs, trash,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Collide(Choice),
    Fat(Fat),
    Delete(trash::Button),
    CopyCancel,
    CopyConfirm,
    NewFileCancel,
    NewFileConfirm,
    Job(u64, jobs::Action),
}

impl Press {
    // A stable number per button, for the element id its pointer state is kept under.
    pub fn code(self) -> u64 {
        match self {
            Press::Collide(c) => c as u64,
            Press::Fat(f) => 0x10 + f as u64,
            Press::Delete(b) => 0x20 + b as u64,
            Press::CopyCancel => 0x30,
            Press::CopyConfirm => 0x31,
            Press::NewFileCancel => 0x32,
            Press::NewFileConfirm => 0x33,
            Press::Job(id, a) => 0x100 + (id << 2) + a as u64,
        }
    }
}
