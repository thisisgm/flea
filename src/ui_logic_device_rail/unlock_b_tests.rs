//! Unlock rows, rail menus and container lookups from lsblk, plus where the QML-only checks went.
use super::devices::parse_devices;
use super::eject::disk_of;
use super::english;
use super::lsblk::device_path;
use super::menu::{rail_menu, MenuRow, RailAction};
use super::unlock::backing_for;
use super::{Group, Kind, RailEntry};

/// A menu row as the catalog reads it: the English label, or a bar between groups.
fn label_of(row: &MenuRow) -> String {
    match row {
        MenuRow::Separator => "|".to_string(),
        MenuRow::Action(a) => a.label_key().and_then(english).unwrap_or_default().to_string(),
    }
}

/// The catalog labels of a menu in order, bars included, joined by commas.
fn labels(entry: &RailEntry) -> String {
    rail_menu(entry).iter().map(label_of).collect::<Vec<_>>().join(",")
}

/// The sample lsblk body: an unlocked crypt on a stick, and a plain stick beside it.
fn open_body() -> String {
    [
        r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","rm":true,"mountpoints":[null],"#,
        r#""children":[{"name":"sdb1","path":"/dev/sdb1","type":"part","fstype":"crypto_LUKS","mountpoints":[null],"#,
        r#""children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","type":"crypt","mountpoints":["/run/media/gm/vault"]}]}]},"#,
        r#"{"name":"sdc","path":"/dev/sdc","type":"disk","rm":true,"mountpoints":[null],"#,
        r#""children":[{"name":"sdc1","path":"/dev/sdc1","type":"part","fstype":"vfat","mountpoints":["/run/media/gm/STICK"]}]}"#,
        r#"]}"#,
    ]
    .concat()
}

#[test]
#[rustfmt::skip]
fn unlocked_crypt_leaf_is_the_one_row_and_locks_its_loop_container() {
    let loop_open = [
        r#"{"blockdevices":[{"name":"cap-loop0","path":"/dev/loop0","mountpoints":[null],"rm":true,"type":"loop","fstype":"crypto_LUKS","#,
        r#""children":[{"name":"luks-6f1c","path":"/dev/mapper/luks-6f1c","label":"S1LUKS","mountpoints":["/run/media/gm/S1LUKS"],"#,
        r#""rm":true,"type":"crypt","fstype":"vfat"}]}]}"#,
    ]
    .concat();
    let rows = parse_devices(&loop_open, false);
    assert!(rows.len() == 1 && rows[0].path == "/run/media/gm/S1LUKS", "the shimmed unlocked leaf is the one row");
    assert_eq!(backing_for(&loop_open, "/dev/mapper/luks-6f1c"), "/dev/loop0", "the shimmed unlocked leaf locks its loop container");
    let judged = disk_of(&loop_open, "/dev/mapper/luks-6f1c").flatten().map(|n| device_path(&n)).unwrap_or_default();
    assert_eq!(judged, "/dev/loop0", "an eject of the shimmed leaf judges its loop disk");
}

#[test]
#[rustfmt::skip]
fn internal_locked_container_is_a_rail_row_too() {
    let internal = [
        r#"{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
        r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null},"#,
        r#"{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":"Backup","mountpoints":[null],"rm":false,"size":500000000000,"type":"part","fstype":"crypto_LUKS","model":null}]}"#,
        r#"]}"#,
    ]
    .concat();
    let inner = parse_devices(&internal, false);
    assert_eq!(inner.len(), 2, "an internal locked container is a rail row too");
    assert!(inner[1].locked, "the internal locked row reads as locked");
}

#[test]
#[rustfmt::skip]
fn locked_rows_offer_unlock_and_removable_ones_flash_image() {
    let removable = RailEntry { label: "Backup".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sdb1".into(), mounted: false, removable: true, locked: true, ..RailEntry::default() };
    let menu = rail_menu(&removable);
    // Catalog text is "Flash Image"; the JS check read "Flash image" (Menu.js). Kept as the Rust catalog states it.
    assert_eq!(labels(&removable), "Unlock,|,Flash Image", "a locked removable row offers Unlock, then Flash image");
    let fixed = RailEntry { label: "Backup".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/nvme0n1p2".into(), mounted: false, removable: false, locked: true, ..RailEntry::default() };
    assert_eq!(rail_menu(&fixed).len(), 1, "a locked internal row offers Unlock alone");
    assert_eq!(menu.first().map(label_of).unwrap_or_default(), "Unlock", "and that row is Unlock");
    assert_eq!(menu[0].action(), Some(RailAction::UnlockVolume), "the Unlock row carries the unlock action");
    assert_eq!(RailAction::UnlockVolume.glyph(), "lock", "the Unlock row draws the lock mark");
}

#[test]
#[rustfmt::skip]
fn unlocked_crypt_row_offers_open_unmount_and_eject_never_unlock() {
    let crypt = RailEntry { label: "Backup".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/dm-0".into(), path: "/run/media/gm/Backup".into(), mounted: true, removable: true, locked: false, ..RailEntry::default() };
    let pairs: Vec<String> = rail_menu(&crypt).iter().filter_map(|r| r.action().map(|a| format!("{}:{}", label_of(r), a.id()))).collect();
    assert_eq!(pairs.join(","), "Open:open,Unmount:unmountVolume,Eject:eject", "an unlocked row offers open, unmount and eject");
    let mapper = RailEntry { label: "vault".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/mapper/luks-vault".into(), path: "/run/media/gm/vault".into(), mounted: true, removable: true, locked: false, ..RailEntry::default() };
    assert!(!(rail_menu(&mapper).iter().filter_map(MenuRow::action).any(|a| a == RailAction::FlashImage)), "an open crypt named by its mapper offers no Flash image");
    let plain = RailEntry { label: "vault".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sdc1".into(), path: "/run/media/gm/vault".into(), mounted: true, removable: true, locked: false, ..RailEntry::default() };
    assert!(rail_menu(&plain).iter().filter_map(MenuRow::action).any(|a| a == RailAction::FlashImage), "while the same row on a partition offers it");
}

#[test]
#[rustfmt::skip]
fn lock_finds_the_container_behind_a_crypt_only() {
    let open = open_body();
    assert_eq!(backing_for(&open, "/dev/mapper/luks-vault"), "/dev/sdb1", "the container behind an unlocked crypt is what lock takes");
    assert_eq!(backing_for(&open, "/dev/sdc1"), "", "a plain partition has no container to lock");
    assert_eq!(backing_for(&open, "/dev/sdb1"), "", "the locked container itself has none either");
    assert_eq!(backing_for(&open, ""), "", "an empty device locks nothing");
    assert_eq!(backing_for("not json", "/dev/mapper/luks-vault"), "", "a body that does not parse locks nothing");
}

// The QML-source checks below are behaviour now, in the GUI crate's tests:
// a typed card keeps its name, the poller ends with its first listing, never starts on a touched card, latches past an erase,
// retries a timeout and reads the remembered rail: crates/flea-gui/src/g2/device_rail/host_tests.rs.
// Tab order field, Cancel, Unlock; Cancel the one stop while unlocking; submit moves to Cancel; a refusal returns to the field;
// an edit clears the error frame, the note and its role; submit holds only the length; open, close and success clear it: crates/flea-gui/src/g2/device_rail/unlock_tests.rs.
// Return, Enter and Space press the focused button and a held repeat never does: crates/flea-gui/src/g2/device_rail/keys_tests.rs.
// The held count is a usize and the mask a count of U+2022, so the QML regex guards have nothing left to guard: crates/flea-gui/src/g2/device_rail/field.rs.
// The eject lock before the device clears, the one refused lock attempt, the judged listing's crypt, the unmount's own listing: crates/flea-gui/src/g2/device_rail/eject_tests.rs.
