//! Network rail menu rows go through the shared menu primitive and resolve stable keys at activation.
use flea::ui_logic_context_menus::{Entry, Text};
use flea::ui_logic_device_rail::{
    menu::{rail_menu, RailAction},
    RailEntry,
};

pub enum Open {
    Direct { path: String, origin: u64 },
    Remote(super::mount::Request),
}
pub fn activate(entry: &RailEntry, origin: u64) -> Open {
    if !entry.path.is_empty() {
        return Open::Direct {
            path: entry.path.clone(),
            origin,
        };
    }
    Open::Remote(super::mount::Request {
        id: 0,
        uri: entry.uri.clone(),
        label: entry.label.clone(),
        origin,
        mounted: entry.mounted,
        authenticated: false,
        password: crate::g2::device_rail::secret::Secret::new(),
    })
}

pub fn unmount(
    entry: &RailEntry,
    gio_listing: &str,
) -> Option<crate::g2::device_rail::process::Job> {
    if !entry.mounted || entry.kind != flea::ui_logic_device_rail::Kind::Share {
        return None;
    }
    let argv = if entry.fstype.starts_with("fuse.") {
        if !super::validate::valid_mount_path(&entry.path) {
            return None;
        }
        vec![
            "fusermount3".into(),
            "-u".into(),
            "--".into(),
            entry.path.clone(),
        ]
    } else {
        vec![
            "gio".into(),
            "mount".into(),
            "-u".into(),
            "--".into(),
            super::validate::release_identity(entry, gio_listing)?,
        ]
    };
    Some(crate::g2::device_rail::process::Job::c_locale(argv))
}

pub fn rows(entry: &RailEntry) -> Vec<Entry> {
    rail_menu(entry)
        .iter()
        .map(|row| {
            if row.action().is_none() {
                return Entry::separator();
            }
            let (id, key, glyph) = match row.action() {
                Some(RailAction::Rename) => ("rename", "network.rename", "rename"),
                Some(RailAction::EditPlace) => ("editPlace", "network.edit_address", "sliders"),
                Some(RailAction::Remove) => ("remove", "network.remove", "minus"),
                Some(RailAction::Unmount) => ("unmount", "device.menu.unmount", "drive"),
                _ => ("open", "device.menu.open", "folder-open"),
            };
            Entry::row(id, Text::Key(key), glyph)
        })
        .collect()
}
