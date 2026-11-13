//! Device, phone and unlock rules for the DEVICES rail, shared by every front end; no I/O here.
pub mod devices;
pub mod eject;
pub mod lsblk;
pub mod menu;
pub mod mounts;
pub mod phones;
pub mod power;
pub mod protocols;
pub mod rail;
pub mod unlock;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod devices_tests;
#[cfg(test)]
mod devices_b_tests;
#[cfg(test)]
mod loop_tests;
#[cfg(test)]
mod mounts_tests;
#[cfg(test)]
mod mounts_b_tests;
#[cfg(test)]
mod phones_tests;
#[cfg(test)]
mod release_tests;
#[cfg(test)]
mod unlock_tests;
#[cfg(test)]
mod unlock_b_tests;

/// Which rail section a row sits in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Group {
    #[default]
    None,
    Device,
    Network,
    Favourite,
}

/// What a rail row stands for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    None,
    Disk,
    Volume,
    Phone,
    Share,
    Dropbox,
    Cloud,
    Favourite,
}

/// One rail row; a volume names its device node, a share or phone its uri.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RailEntry {
    pub path: String,
    pub label: String,
    pub group: Group,
    pub kind: Kind,
    pub device: String,
    pub uri: String,
    pub mounted: bool,
    pub removable: bool,
    pub media_removable: bool,
    pub read_only: bool,
    pub size: Option<u64>,
    pub glyph: String,
    pub locked: bool,
    pub fstype: String,
    pub fsver: String,
    pub uuid: String,
    pub loop_device: bool,
    pub editable: Option<bool>,
}

/// A sentence by catalog key with its arguments; count selects a plural form and is not an argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub key: &'static str,
    pub args: Vec<String>,
    pub count: Option<i64>,
}

impl Note {
    pub fn new(key: &'static str, args: &[&str]) -> Self {
        Self { key, args: args.iter().map(|a| a.to_string()).collect(), count: None }
    }

    /// Plain data shown as is, such as a drive label standing alone.
    pub fn plain(text: &str) -> Self {
        Self { key: "", args: vec![text.to_string()], count: None }
    }

    /// The English rendering, for tests and logs; front ends translate through the catalog.
    pub fn english(&self) -> String {
        let template = match self.count {
            Some(n) => plural_english(self.key, n),
            None => english(self.key),
        };
        let Some(template) = template else { return self.args.join(" ") };
        let mut values: Vec<String> = Vec::new();
        if let Some(n) = self.count {
            values.push(n.to_string());
        }
        values.extend(self.args.iter().cloned());
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        crate::i18n::substitute(template, &refs)
    }
}

/// The English catalog text under the device prefix, mirrored from i18n/en/flea.json.
pub const ENGLISH: &[(&str, &str)] = &[
    ("device.nothing_to_release", "%1 has nothing to eject or unmount"),
    ("device.still_ejecting", "Still ejecting %1; wait for its result"),
    ("device.still_unmounting", "Still unmounting %1; wait for its result"),
    ("device.another_mounting", "Another drive is still mounting. Try again when it finishes"),
    ("device.still_unmounting_try_again", "%1 is still unmounting. Try again when it finishes"),
    ("device.ejecting", "Ejecting %1. Do not unplug it yet."),
    ("device.no_folder", "%1 was mounted, but Flea could not find its folder."),
    ("device.eject_unfinished", "%1 did not finish ejecting and is still mounted."),
    ("device.mount_failed", "%1 could not be mounted."),
    ("device.damaged", "%1 could not be mounted: its file system is damaged or not supported."),
    ("device.unmount_failed", "%1 could not be unmounted."),
    ("device.eject_failed", "%1 could not be ejected."),
    ("device.busy", "%1 is busy; close what is using it and try again."),
    ("device.mount_refused", "%1 was not mounted: you do not have permission."),
    ("device.unmount_refused", "%1 was not unmounted: you do not have permission."),
    ("device.eject_refused", "%1 was not ejected: you do not have permission."),
    ("device.no_disc", "%1 has no disc or card in it."),
    ("device.not_mountable", "%1 cannot be mounted on this computer."),
    ("device.transfer_using", "%1 could not be unmounted. A transfer may still be using it."),
    (
        "device.copy_running",
        "A copy on %1 is still running. Try again when it finishes.",
    ),
    (
        "device.move_running",
        "A move on %1 is still running. Try again when it finishes.",
    ),
    (
        "device.archive_running",
        "An archive on %1 is still running. Try again when it finishes.",
    ),
    (
        "device.send_running",
        "A send on %1 is still running. Try again when it finishes.",
    ),
    (
        "device.action_running",
        "An action on %1 is still running. Try again when it finishes.",
    ),
    ("device.system_drive", "%1 is the system drive, which Flea never ejects."),
    ("device.system_volume", "This folder is not on a drive Flea can eject."),
    ("device.no_volume", "This folder is not on a drive Flea can eject."),
    ("device.safe", "Ejected %1. It is safe to unplug."),
    ("device.safe_image", "Ejected %1."),
    ("device.still_mounted", "%1 is still mounted; close what is using it."),
    ("device.unconfirmed", "Could not confirm %1 was ejected; do not unplug it yet."),
    ("device.loop_attached", "Unmounted %1, but its loop device is still attached."),
    ("device.unlock.title", "Unlock %1"),
    ("device.unlock.unlocking", "Unlocking %1…"),
    ("device.unlock.wrong", "That passphrase did not work. Check Caps Lock."),
    ("device.unlock.unlocked", "Unlocked and mounted %1."),
    ("device.unlock.lock_failed", "%1 was unmounted but could not be locked; close what is using it."),
    ("device.unlock.mount_failed", "%1 was unlocked but could not be mounted."),
    ("device.unlock.failed", "%1 could not be unlocked."),
    ("device.unlock.subtitle_three", "%1 · %2 · %3"),
    ("device.unlock.subtitle_two", "%1 · %2"),
    ("device.unlock.passphrase", "Passphrase"),
    ("device.unlock.recovery_key", "Password or recovery key"),
    ("device.unlock.wrong_bitlocker", "That password or recovery key did not work."),
    ("device.unlock.button", "Unlock"),
    ("device.unlock.cancel", "Cancel"),
    ("device.menu.open", "Open"),
    ("device.menu.unlock", "Unlock"),
    ("device.menu.mount", "Mount"),
    ("device.menu.unmount", "Unmount"),
    ("device.menu.eject", "Eject"),
    ("device.menu.flash_image", "Flash Image"),
];

/// Plural catalog entries under the device prefix: key, one, other.
pub const ENGLISH_PLURAL: &[(&str, &str, &str)] = &[(
    "device.blocked",
    "%2 could not be ejected because %3 on the same drive is still mounted. Eject that instead.",
    "%2 could not be ejected because %3 on the same drive are still mounted. Eject those instead.",
)];

pub fn english(key: &str) -> Option<&'static str> {
    ENGLISH.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

fn plural_english(key: &str, count: i64) -> Option<&'static str> {
    let (_, one, other) = ENGLISH_PLURAL.iter().find(|(k, _, _)| *k == key)?;
    Some(if count == 1 { one } else { other })
}
