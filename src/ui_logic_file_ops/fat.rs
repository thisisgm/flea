// The FAT names card a copy onto a FAT or NTFS volume asks before names that volume cannot hold.
use super::{count, n};
use crate::i18n::I18n;

// The FAT names card: Rename and copy is the default; the order is the card's left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fat {
    Cancel,
    Skip,
    Rename,
}

pub const FAT_BUTTONS: [Fat; 3] = [Fat::Cancel, Fat::Skip, Fat::Rename];
pub const FAT_START: Fat = Fat::Rename;

impl Fat {
    pub fn wire(self) -> &'static str {
        match self {
            Fat::Cancel => "cancel",
            Fat::Skip => "skip",
            Fat::Rename => "rename",
        }
    }
    pub fn label_key(self) -> &'static str {
        match self {
            Fat::Cancel => "jobs.button.cancel",
            Fat::Skip => "jobs.fat.skip",
            Fat::Rename => "jobs.fat.rename",
        }
    }
}

// The mount table's own word as the card spells it; vfat could be any FAT width, so it claims none.
pub fn filesystem_label(fstype: &str) -> String {
    match fstype.to_ascii_lowercase().as_str() {
        "exfat" => "exFAT".into(),
        "vfat" | "fat" | "msdos" => "FAT".into(),
        "fat12" => "FAT12".into(),
        "fat16" => "FAT16".into(),
        "fat32" => "FAT32".into(),
        "ntfs" | "ntfs3" => "NTFS".into(),
        _ => fstype.into(),
    }
}

pub fn fat_title(catalog: &I18n, total: u64, volume: &str, fstype: &str) -> String {
    catalog.trn_args(
        "jobs.fat.title",
        n(total),
        &[volume, &filesystem_label(fstype)],
    )
}

pub fn fat_note(catalog: &I18n, fstype: &str) -> String {
    catalog.tr_args("jobs.fat.note", &[&filesystem_label(fstype)])
}

pub fn grouped(catalog: &I18n, value: u64) -> String {
    count(catalog, value)
}
