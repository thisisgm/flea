//! The rail menu's rows for one device, phone or share row, in inventory order with group separators.
use super::{Group, Kind, RailEntry};

/// One rail menu action, in the order the menu lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailAction {
    Open,
    UnlockVolume,
    MountVolume,
    MountPhone,
    UnmountVolume,
    UnmountPhone,
    Unmount,
    Eject,
    FlashImage,
    Rename,
    EditPlace,
    Remove,
    RemoveFavourite,
}

/// Every rail action in menu order.
pub const INVENTORY: [RailAction; 13] = [
    RailAction::Open,
    RailAction::UnlockVolume,
    RailAction::MountVolume,
    RailAction::MountPhone,
    RailAction::UnmountVolume,
    RailAction::UnmountPhone,
    RailAction::Unmount,
    RailAction::Eject,
    RailAction::FlashImage,
    RailAction::Rename,
    RailAction::EditPlace,
    RailAction::Remove,
    RailAction::RemoveFavourite,
];

impl RailAction {
    /// The action id the menu and the rail share.
    pub fn id(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::UnlockVolume => "unlockVolume",
            Self::MountVolume => "mountVolume",
            Self::MountPhone => "mountPhone",
            Self::UnmountVolume => "unmountVolume",
            Self::UnmountPhone => "unmountPhone",
            Self::Unmount => "unmount",
            Self::Eject => "eject",
            Self::FlashImage => "flashImage",
            Self::Rename => "rename",
            Self::EditPlace => "editPlace",
            Self::Remove => "remove",
            Self::RemoveFavourite => "removeFavourite",
        }
    }

    /// The mark drawn beside the row.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Open => "folder-open",
            Self::UnlockVolume => "lock",
            Self::Eject => "eject",
            Self::FlashImage => "file",
            Self::Rename => "rename",
            Self::EditPlace => "sliders",
            Self::Remove | Self::RemoveFavourite => "minus",
            _ => "drive",
        }
    }

    /// The separator group; a change of group draws a rule above the row.
    pub fn group(self) -> &'static str {
        match self {
            Self::Open | Self::UnlockVolume | Self::MountVolume | Self::MountPhone => "open",
            Self::UnmountVolume | Self::UnmountPhone | Self::Unmount | Self::Eject => "rrelease",
            Self::FlashImage => "rflash",
            Self::Rename | Self::EditPlace => "basic",
            Self::Remove => "rremove",
            Self::RemoveFavourite => "inspect",
        }
    }

    /// The catalog key for device rows; share and favourite rows belong to other units' prefixes.
    pub fn label_key(self) -> Option<&'static str> {
        Some(match self {
            Self::Open => "device.menu.open",
            Self::UnlockVolume => "device.menu.unlock",
            Self::MountVolume | Self::MountPhone => "device.menu.mount",
            Self::UnmountVolume | Self::UnmountPhone | Self::Unmount => "device.menu.unmount",
            Self::Eject => "device.menu.eject",
            Self::FlashImage => "device.menu.flash_image",
            _ => return None,
        })
    }

    /// Whether the row offers this action; the rail entry alone decides.
    pub fn available(self, entry: &RailEntry) -> bool {
        let volume = entry.group == Group::Device && entry.kind == Kind::Volume;
        let phone = entry.group == Group::Device && entry.kind == Kind::Phone;
        let share = entry.group == Group::Network && entry.kind == Kind::Share;
        let mounted = entry.mounted;
        let editable = entry.editable != Some(false);
        match self {
            Self::UnlockVolume => volume && entry.locked,
            Self::MountVolume => volume && !mounted && !entry.locked,
            Self::MountPhone => phone && !mounted,
            Self::Open => (volume || phone || share) && mounted,
            Self::UnmountVolume => volume && mounted,
            Self::UnmountPhone => phone && mounted,
            Self::Unmount => share && mounted,
            Self::Eject => volume && mounted && (entry.removable || entry.loop_device),
            Self::FlashImage => {
                let mapper = entry.device.starts_with("/dev/mapper/") || entry.device.starts_with("/dev/dm-");
                volume && entry.removable && !entry.read_only && !mapper
            }
            Self::Rename | Self::EditPlace | Self::Remove => share && editable,
            Self::RemoveFavourite => entry.kind == Kind::Favourite,
        }
    }
}

/// One menu row: an action or the rule between two groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuRow {
    Action(RailAction),
    Separator,
}

impl MenuRow {
    pub fn action(&self) -> Option<RailAction> {
        match self {
            Self::Action(a) => Some(*a),
            Self::Separator => None,
        }
    }
}

/// The rows a rail entry's menu offers, a separator wherever the group changes.
pub fn rail_menu(entry: &RailEntry) -> Vec<MenuRow> {
    let mut out = Vec::new();
    let mut group = "";
    for action in INVENTORY.into_iter().filter(|a| a.available(entry)) {
        if !out.is_empty() && group != action.group() {
            out.push(MenuRow::Separator);
        }
        group = action.group();
        out.push(MenuRow::Action(action));
    }
    out
}
