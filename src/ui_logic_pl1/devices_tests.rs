//! Ported from tests/js/devices.js: live lsblk rows, USB bridges, labels, capacities and empty listings.
use super::devices::{is_system_path, parse_devices};
use super::lsblk::bytes;
use super::menu::{rail_menu, MenuRow};
use super::power::eject_command;
use super::{Group, Kind, RailEntry};
use crate::jsondoc::Json;

// The live box's own listing, taken 2026-09-11 with one stick in; the encrypted root lists four mountpoints.
pub(super) const LIVE: &str = concat!(
    r#"{"blockdevices":["#,
    r#"{"name":"loop0","path":"/dev/loop0","label":"FLEATEST","mountpoints":[null],"rm":false,"size":67108864,"type":"loop","model":null},"#,
    r#"{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk","#,
    r#""children":[{"name":"sda1","path":"/dev/sda1","label":"128GB","mountpoints":["/run/media/gm/128GB"],"rm":true,"size":124656812032,"type":"part","model":null}]},"#,
    r#"{"name":"zram0","path":"/dev/zram0","label":"zram0","mountpoints":["[SWAP]"],"rm":false,"size":20724056064,"type":"disk","model":null},"#,
    r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
    r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/boot"],"rm":false,"size":2147483648,"type":"part","model":null},"#,
    r#"{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":null,"mountpoints":[null],"rm":false,"size":253910581248,"type":"part","model":null,"#,
    r#""children":[{"name":"root","path":"/dev/mapper/root","label":null,"mountpoints":["/home","/var/log","/"],"rm":false,"size":253893804032,"type":"crypt","model":null}]}]}"#,
    r#"]}"#,
);

// A stick present and unmounted, the state it sits in on a box that automounts nothing.
pub(super) const UNMOUNTED: &str = concat!(
    r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk","#,
    r#""children":[{"name":"sda1","path":"/dev/sda1","label":"128GB","mountpoints":[null],"rm":true,"size":124656812032,"type":"part","model":null,"fstype":"vfat"}]}]}"#,
);

fn field_list(rows: &[RailEntry], field: fn(&RailEntry) -> &str) -> String {
    rows.iter().map(field).collect::<Vec<_>>().join(",")
}

fn menu_ids(entry: &RailEntry) -> Vec<&'static str> {
    rail_menu(entry).iter().filter_map(MenuRow::action).map(|a| a.id()).collect()
}

fn size_node(value: Option<Json>) -> Json {
    match value {
        None => Json::Obj(Vec::new()),
        Some(v) => Json::Obj(vec![("size".to_string(), v)]),
    }
}

#[test]
fn live_box_rows_match_the_listing() {
    let d = parse_devices(LIVE, false);
    assert_eq!(d.len(), 2, "the live box has one internal disk and one removable volume");
    assert_eq!(d[0].kind, Kind::Disk, "the internal disk sorts first");
    assert_eq!(d[0].label, "nvme0n1", "the internal disk is named by its kernel name");
    assert_eq!(d[0].path, "/", "the internal disk row opens the root of the filesystem");
    assert!(d[0].mounted, "the internal disk is always mounted");
    assert_eq!(d[0].size, Some(256060514304), "the internal disk retains raw capacity bytes");
    assert_eq!(d[0].device, "/dev/nvme0n1", "zram is not the internal disk");
    assert_eq!(d[1].label, "128GB", "the removable volume takes the filesystem label");
    assert_eq!(d[1].device, "/dev/sda1", "the removable volume carries its device node for gio");
    assert_eq!(d[1].path, "/run/media/gm/128GB", "a mounted volume carries the mountpoint lsblk reported");
    assert!(d[1].mounted, "a volume with a mountpoint reads as mounted");
    assert!(d[1].removable, "a removable volume is marked removable, which is what offers it Eject");
    assert_eq!(field_list(&d, |e| e.label.as_str()), "nvme0n1,128GB", "a loop device is not a device row");
    assert!(!d[1].loop_device, "a real removable volume is not marked loop");
}

#[test]
fn whole_stick_is_a_row_and_ejects_by_its_mountpoint() {
    let whole_stick = r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","type":"disk","rm":true,"tran":"usb","fstype":"iso9660","label":"WHOLE_STICK","mountpoints":["/run/media/user/WHOLE_STICK"],"children":[{"name":"sdb1","path":"/dev/sdb1","type":"part","fstype":"iso9660","mountpoints":["/run/media/user/CHILD"],"rm":true},{"name":"sdb2","path":"/dev/sdb2","type":"part","mountpoints":[null],"rm":true}]}]}"#;
    let rows = parse_devices(whole_stick, false);
    let whole = rows.iter().find(|e| e.device == "/dev/sdb");
    assert_eq!(rows.len(), 2, "a stick mounted whole is a row even with a partition table");
    assert_eq!(whole.map(|e| e.path.as_str()).unwrap_or("missing"), "/run/media/user/WHOLE_STICK", "the whole stick keeps its mountpoint");
    let eject = whole.map(|e| eject_command(e).join(" ")).unwrap_or_else(|| "missing".to_string());
    assert_eq!(eject, "gio mount -e /run/media/user/WHOLE_STICK", "the whole stick still ejects through gio mount -e");
    assert!(rows.iter().any(|e| e.device == "/dev/sdb1"), "the mounted whole stick still recurses into its mounted child");
}

#[test]
fn system_disk_partitions_are_not_rows() {
    // The system disk is the one place the walk never goes: the disk row already stands for it.
    let d = parse_devices(LIVE, false);
    assert_eq!(field_list(&d, |e| e.path.as_str()), "/,/run/media/gm/128GB", "no partition of the system disk becomes a row of its own");
}

#[test]
fn second_internal_drive_is_a_mounted_volume_row() {
    let two = parse_devices(
        concat!(
            r#"{"blockdevices":["#,
            r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
            r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
            r#"{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"size":2000398934016,"type":"disk","model":"Samsung SSD 870","#,
            r#""children":[{"name":"sdb1","path":"/dev/sdb1","label":"Vault","mountpoints":["/mnt/vault"],"rm":false,"size":2000398934016,"type":"part","model":null}]}"#,
            r#"]}"#,
        ),
        false,
    );
    assert_eq!(two.len(), 2, "a second internal drive is a row, which it was not before 0.2.1");
    assert_eq!(two[1].label, "Vault", "and it is the mounted volume, not the whole disk");
    assert_eq!(two[1].path, "/mnt/vault", "it opens where it is mounted");
    assert!(two[1].mounted, "it reads as mounted");
    assert!(!two[1].removable, "and it is not removable, so nothing offers to eject a fixed disk");
    let fixed = RailEntry { group: Group::Device, kind: Kind::Volume, mounted: true, removable: false, ..RailEntry::default() };
    assert_eq!(menu_ids(&fixed), vec!["open", "unmountVolume"], "a fixed internal volume offers Open and Unmount");
}

#[test]
fn disk_row_is_the_one_holding_root_not_the_first_listed() {
    let order = parse_devices(
        concat!(
            r#"{"blockdevices":["#,
            r#"{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":false,"size":2000398934016,"type":"disk","model":"Data","#,
            r#""children":[{"name":"sda1","path":"/dev/sda1","label":"Data","mountpoints":["/mnt/data"],"rm":false,"size":2000398934016,"type":"part","model":null}]},"#,
            r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
            r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]}"#,
            r#"]}"#,
        ),
        false,
    );
    assert_eq!(order[0].device, "/dev/nvme0n1", "the disk row is the one that carries /, not the one lsblk listed first");
    assert_eq!(order[1].path, "/mnt/data", "and the other internal drive is a volume row beside it");
}

#[test]
fn usb_bridge_is_removable_however_its_rm_column_reads() {
    // Shape captured from the PR 74 WD My Passport: a USB bridge reports rm=false on the disk.
    let bridge = concat!(
        r#"{"blockdevices":["#,
        r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"tran":"nvme","size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
        r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"tran":null,"size":256060514304,"type":"part","model":null}]},"#,
        r#"{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"tran":"usb","size":2000398934016,"type":"disk","model":"My Passport 25E2","#,
        r#""children":[{"name":"sdb1","path":"/dev/sdb1","label":"Passport","mountpoints":["/run/media/gm/Passport"],"rm":false,"tran":null,"size":2000398934016,"type":"part","model":null,"fstype":"vfat"}]}"#,
        r#"]}"#,
    );
    let usb = parse_devices(bridge, false);
    assert_eq!(format!("{}|{}", usb.len(), usb[1].label), "2|Passport", "a mounted USB bridge is a rail row");
    assert!(usb[1].removable, "and it is removable however its RM column reads, because it unplugs");
    assert_eq!(menu_ids(&usb[1]), vec!["open", "unmountVolume", "eject", "flashImage"], "so the rail offers it Open, Unmount, Eject and Flash image");
    assert_eq!(parse_devices(&bridge.replace(r#"["/run/media/gm/Passport"]"#, "[null]"), false).len(), 2, "an unmounted USB bridge is a row too");
    assert!(!usb[0].removable, "the system disk is not offered an eject");
}

#[test]
fn unlocked_usb_volume_stays_one_removable_row() {
    let locked = parse_devices(
        concat!(
            r#"{"blockdevices":["#,
            r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":["/"],"rm":false,"tran":"nvme","size":256060514304,"type":"disk","model":"KBG40ZNS256G"},"#,
            r#"{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"tran":"usb","size":8589934592,"type":"disk","model":"Vault Drive","#,
            r#""children":[{"name":"sdb1","path":"/dev/sdb1","label":null,"mountpoints":[null],"rm":false,"tran":null,"size":8589934592,"type":"part","model":null,"#,
            r#""children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","label":"vault","mountpoints":["/run/media/gm/vault"],"rm":false,"tran":null,"size":8589934592,"type":"crypt","model":null}]}]}"#,
            r#"]}"#,
        ),
        false,
    );
    let last = &locked[locked.len() - 1];
    assert_eq!(format!("{}|{}", locked.len(), last.label), "2|vault", "an unlocked volume on a USB drive is the one row, as it always was");
    assert!(last.removable, "and it is marked removable, which is what offers the unlocked row Eject");
}

#[test]
fn unmounted_internal_partition_and_swap_are_not_rows() {
    let spare = concat!(
        r#"{"blockdevices":["#,
        r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
        r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
        r#"{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"size":2000398934016,"type":"disk","model":"Spare","#,
        r#""children":[{"name":"sdb1","path":"/dev/sdb1","label":"RECOVERY","mountpoints":[null],"rm":false,"size":2000398934016,"type":"part","model":null}]}"#,
        r#"]}"#,
    );
    assert_eq!(parse_devices(spare, false).len(), 1, "an unmounted internal partition is not a row");
    let swap_disk = concat!(
        r#"{"blockdevices":["#,
        r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
        r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
        r#"{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":false,"size":17179869184,"type":"disk","model":"Swap","#,
        r#""children":[{"name":"sdb1","path":"/dev/sdb1","label":null,"mountpoints":["[SWAP]"],"rm":false,"size":17179869184,"type":"part","model":null}]}"#,
        r#"]}"#,
    );
    assert_eq!(parse_devices(swap_disk, false).len(), 1, "a swap partition on another drive is not a row");
}

#[test]
fn glib_system_paths_hide_with_the_system_mounts() {
    assert!(is_system_path("/run"), "a volume mounted only at /run hides with the system mounts");
    assert!(is_system_path("/boot"), "a volume mounted only at /boot hides with the system mounts");
    assert!(!is_system_path("/efi"), "/efi is no system path, so a non-ESP volume there stays a row");
    assert!(!is_system_path("/boot/efi"), "/boot/efi is no system path either");
}

#[test]
fn unlocked_encrypted_stick_is_one_row() {
    // Only a leaf is a volume, so the partition and its unlocked crypt do not both appear.
    let opened = parse_devices(
        concat!(
            r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":"Stick","#,
            r#""children":[{"name":"sda1","path":"/dev/sda1","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null,"#,
            r#""children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","label":"vault","mountpoints":["/run/media/gm/vault"],"rm":false,"size":8589934592,"type":"crypt","model":null}]}]}]}"#,
        ),
        false,
    );
    assert_eq!(opened.len(), 1, "an unlocked encrypted stick is one row, not two");
    assert_eq!(opened[0].label, "vault", "and the row is the crypt, which is the thing that mounted");
    assert_eq!(opened[0].device, "/dev/mapper/luks-vault", "a device-mapper volume carries the path gio can actually act on");
}

#[test]
fn empty_or_garbage_listing_parses_to_nothing() {
    assert_eq!(parse_devices("", false).len(), 0, "empty lsblk output parses to nothing");
    assert_eq!(parse_devices("not json at all\n", false).len(), 0, "garbage lsblk output parses to nothing");
    assert_eq!(parse_devices("{}", false).len(), 0, "valid json with no blockdevices key parses to nothing");
    assert_eq!(parse_devices(r#"{"blockdevices":[]}"#, false).len(), 0, "an empty blockdevices array parses to nothing");
}

#[test]
fn capacity_values_stay_absent_unless_safe_integers() {
    // JS Infinity and NaN are held as the number text "Infinity" and "NaN", the form the Json tree keeps.
    let bad = vec![
        size_node(None),
        size_node(Some(Json::Null)),
        size_node(Some(Json::Str(String::new()))),
        size_node(Some(Json::Str("116.1G".into()))),
        size_node(Some(Json::Str("256060514304".into()))),
        size_node(Some(Json::Num("-1".into()))),
        size_node(Some(Json::Num("1.5".into()))),
        size_node(Some(Json::Num("Infinity".into()))),
        size_node(Some(Json::Num("NaN".into()))),
        size_node(Some(Json::Num("9007199254740992".into()))),
    ];
    assert!(bad.iter().all(|n| bytes(n).is_none()), "invalid capacity values stay absent rather than becoming display text");
    assert_eq!(bytes(&size_node(Some(Json::Num("0".into())))), Some(0), "zero-byte devices retain their real numeric size");
}

#[test]
fn unmounted_stick_is_still_a_row() {
    let u = parse_devices(UNMOUNTED, false);
    assert_eq!(u.len(), 1, "an unmounted stick is still a row");
    assert!(!u[0].mounted, "an unmounted volume reads as unmounted");
    assert_eq!(u[0].path, "", "an unmounted volume has no path to open yet");
    assert_eq!(u[0].device, "/dev/sda1", "an unmounted volume still carries the device node its mount needs");
    assert_eq!(u[0].size, Some(124656812032), "an unmounted volume keeps its capacity separate from its label");
}
