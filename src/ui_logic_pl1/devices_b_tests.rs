//! Ported from tests/js/devices.js: optical and multi-drive rows, labels, eject verdicts, power-off queues and siblings.
use super::devices::{mount_timer_start, parse_devices};
use super::devices_tests::{LIVE, UNMOUNTED};
use super::eject::{blockers, sentence, verdict, Verdict};
use super::menu::{rail_menu, MenuRow};
use super::mounts::parse_mounts;
use super::power::{drives_holding, power_off_queue, sys_base, written_sectors, Queued};
use super::{Kind, RailEntry};

// Four rows from two sticks, the system disk and one bare internal drive; the nvme1n1 drive adds none.
const MANY: &str = concat!(
    r#"{"blockdevices":["#,
    r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
    r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
    r#"{"name":"nvme1n1","path":"/dev/nvme1n1","label":null,"mountpoints":[null],"rm":false,"size":1000000000000,"type":"disk","model":"Second NVMe"},"#,
    r#"{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk","#,
    r#""children":[{"name":"sda1","path":"/dev/sda1","label":"first","mountpoints":["/run/media/gm/first"],"rm":true,"size":62277025792,"type":"part","model":null},"#,
    r#"{"name":"sda2","path":"/dev/sda2","label":"second","mountpoints":[null],"rm":true,"size":62277025792,"type":"part","model":null,"fstype":"vfat"}]},"#,
    r#"{"name":"sdb","path":"/dev/sdb","label":"CARD","mountpoints":[null],"rm":true,"size":34359738368,"type":"disk","model":"SD Reader","fstype":"vfat"}"#,
    r#"]}"#,
);

// An encrypted volume on an unmounted partition, the crypt child carries the mount.
const CRYPT: &str = concat!(
    r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,"#,
    r#""children":[{"name":"sda1","path":"/dev/sda1","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null,"#,
    r#""children":[{"name":"luks-vault","path":"/dev/mapper/luks-vault","label":"vault","mountpoints":["/run/media/gm/vault"],"rm":false,"size":8589934592,"type":"crypt","model":null}]}]}]}"#,
);

// A real multi-boot system disk with its live GPT parttype GUIDs and usable Windows data volumes.
const MULTIBOOT: &str = concat!(
    r#"{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[],"rm":false,"tran":"nvme","size":512110190592,"type":"disk","model":"Micron_2450","#,
    r#""children":["#,
    r#"{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":"ESP","mountpoints":[],"rm":false,"size":272629760,"type":"part","model":null,"fstype":"vfat","parttypename":"EFI System","parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b"},"#,
    r#"{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":null,"mountpoints":[],"rm":false,"size":16777216,"type":"part","model":null,"fstype":null,"parttypename":"Microsoft reserved","parttype":"e3c9e316-0b5c-4db8-817d-f92df00215ae"},"#,
    r#"{"name":"nvme0n1p3","path":"/dev/nvme0n1p3","label":"Acer","mountpoints":[],"rm":false,"size":221029335040,"type":"part","model":null,"fstype":"ntfs","parttypename":"Microsoft basic data","parttype":"ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"},"#,
    r#"{"name":"nvme0n1p4","path":"/dev/nvme0n1p4","label":null,"mountpoints":[],"rm":false,"size":53491007488,"type":"part","model":null,"fstype":"ntfs","parttypename":"Microsoft basic data","parttype":"ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"},"#,
    r#"{"name":"nvme0n1p5","path":"/dev/nvme0n1p5","label":"OMARCHY_EFI","mountpoints":["/boot"],"rm":false,"size":2147484160,"type":"part","model":null,"fstype":"vfat","parttypename":"EFI System","parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b"},"#,
    r#"{"name":"nvme0n1p6","path":"/dev/nvme0n1p6","label":null,"mountpoints":[],"rm":false,"size":158911693312,"type":"part","model":null,"fstype":"crypto_LUKS","parttypename":"Linux filesystem","parttype":"0fc63daf-8483-4772-8e79-3d69d8477de4","#,
    r#""children":[{"name":"omarchy_root","path":"/dev/mapper/omarchy_root","label":"OMARCHY","mountpoints":["/var/cache/pacman/pkg","/var/log","/home","/"],"rm":false,"size":158894916096,"type":"crypt","model":null,"fstype":"btrfs"}]},"#,
    r#"{"name":"nvme0n1p7","path":"/dev/nvme0n1p7","label":"Recovery","mountpoints":[],"rm":false,"size":1073741824,"type":"part","model":null,"fstype":"ntfs","parttypename":"EFI System","parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b"},"#,
    r#"{"name":"nvme0n1p8","path":"/dev/nvme0n1p8","label":"RYOKUBOOT","mountpoints":[],"rm":false,"size":2147483648,"type":"part","model":null,"fstype":"vfat","parttypename":"Linux extended boot","parttype":"bc13c2ff-59e6-4262-a352-b275fd6f7172"},"#,
    r#"{"name":"nvme0n1p9","path":"/dev/nvme0n1p9","label":"ryoku","mountpoints":[],"rm":false,"size":73015492608,"type":"part","model":null,"fstype":"btrfs","parttypename":"Linux filesystem","parttype":"0fc63daf-8483-4772-8e79-3d69d8477de4"}"#,
    r#"]}]}"#,
);

// The shared dm node appears under two partitions, the second partition also holds a second dm node.
const MAPPED: &str = concat!(
    r#"{"blockdevices":[{"name":"sdc","path":"/dev/sdc","mountpoints":[null],"type":"disk","children":["#,
    r#"{"name":"sdc1","path":"/dev/sdc1","mountpoints":[null],"type":"part","children":[{"name":"dm-0","path":"/dev/mapper/vg-data","mountpoints":["/mnt/data"],"type":"lvm","uuid":"shared"}]},"#,
    r#"{"name":"sdc2","path":"/dev/sdc2","mountpoints":[null],"type":"part","children":[{"name":"dm-0","path":"/dev/mapper/vg-data","mountpoints":["/mnt/data"],"type":"lvm","uuid":"shared"},"#,
    r#"{"name":"dm-1","path":"/dev/mapper/vg-other","mountpoints":["/mnt/other"],"type":"lvm","uuid":"shared"}]}]}]}"#,
);

// Two mounted volumes and one unmounted partition on sda, a mounted /boot ESP, and a mounted disk on sdb.
const VOLS: &str = concat!(
    r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","mountpoints":[null],"type":"disk","children":["#,
    r#"{"name":"sda1","path":"/dev/sda1","label":"EFI","mountpoints":["/run/media/u/EFI"],"fstype":"vfat","type":"part","parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b"},"#,
    r#"{"name":"sda2","path":"/dev/sda2","label":"DATA","mountpoints":["/run/media/u/DATA"],"fstype":"hfsplus","type":"part"},"#,
    r#"{"name":"sda3","path":"/dev/sda3","label":null,"mountpoints":[null],"fstype":"vfat","type":"part"},"#,
    r#"{"name":"sda4","path":"/dev/sda4","label":null,"mountpoints":["/boot"],"fstype":"vfat","type":"part"}]},"#,
    r#"{"name":"sdb","path":"/dev/sdb","mountpoints":["/run/media/u/c"],"type":"disk"}]}"#,
);

fn field_list(rows: &[RailEntry], field: fn(&RailEntry) -> &str) -> String {
    rows.iter().map(field).collect::<Vec<_>>().join(",")
}

fn menu_ids(entry: &RailEntry) -> Vec<&'static str> {
    rail_menu(entry).iter().filter_map(MenuRow::action).map(|a| a.id()).collect()
}

fn rail_volumes() -> Vec<RailEntry> {
    let volume = |device: &str, path: &str, mounted: bool| RailEntry {
        kind: Kind::Volume,
        device: device.into(),
        path: path.into(),
        mounted,
        ..RailEntry::default()
    };
    vec![
        volume("/dev/sda1", "/run/media/u/a", true),
        volume("/dev/sda2", "/run/media/u/b", true),
        volume("/dev/sdb1", "/run/media/u/c", true),
        volume("/dev/sda3", "", false),
        RailEntry { kind: Kind::Disk, device: "/dev/sda".into(), path: "/".into(), mounted: true, ..RailEntry::default() },
    ]
}

fn rail_queue() -> Vec<Queued> {
    vec![
        Queued { device: "/dev/sda1".into(), path: "/run/media/u/a".into() },
        Queued { device: "/dev/sda2".into(), path: "/run/media/u/b".into() },
    ]
}

#[test]
fn optical_drive_rows_follow_disc_state() {
    let head = r#"{"blockdevices":["#;
    let system = concat!(
        r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
        r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
    );
    let usb = r#"{"name":"sdb","path":"/dev/sdb","label":"USB","mountpoints":[null],"rm":true,"size":34359738368,"type":"disk","model":"USB Flash Disk","fstype":"vfat"}"#;
    let empty_rom = r#"{"name":"sr0","path":"/dev/sr0","label":null,"mountpoints":[null],"rm":true,"size":0,"type":"rom","model":"MATSHITA DVD+/-RW UJ8FB"},"#;
    let inserted_rom = r#"{"name":"sr0","path":"/dev/sr0","label":null,"mountpoints":[null],"rm":true,"size":0,"type":"rom","fstype":"iso9660","model":"MATSHITA DVD+/-RW UJ8FB"},"#;
    let mounted_rom = r#"{"name":"sr0","path":"/dev/sr0","label":null,"mountpoints":["/run/media/gm/DVD"],"rm":true,"size":0,"type":"rom","fstype":"iso9660","model":"MATSHITA DVD+/-RW UJ8FB"},"#;
    let tail = "]}";
    let empty = parse_devices(&format!("{head}{system}{empty_rom}{usb}{tail}"), false);
    assert_eq!(field_list(&empty, |e| e.label.as_str()), "nvme0n1,USB", "an empty optical drive is hidden while the unmounted USB disk remains");
    let inserted = parse_devices(&format!("{head}{system}{inserted_rom}{usb}{tail}"), false);
    assert_eq!(field_list(&inserted, |e| e.label.as_str()), "nvme0n1,MATSHITA DVD+/-RW UJ8FB,USB", "an inserted optical disc remains a row");
    assert!(!inserted[1].mounted, "an inserted optical disc is still unmounted");
    let mounted = parse_devices(&format!("{head}{system}{mounted_rom}{usb}{tail}"), false);
    assert_eq!(mounted[1].label, "MATSHITA DVD+/-RW UJ8FB", "a mounted optical disc remains a row");
    assert_eq!(mounted[1].path, "/run/media/gm/DVD", "a mounted optical disc keeps its published path");
}

#[test]
fn several_drives_give_one_disk_row_and_their_volumes() {
    let m = parse_devices(MANY, false);
    assert_eq!(m.len(), 4, "four rows come out of two sticks, the system disk and one bare internal drive");
    assert_eq!(m[0].label, "nvme0n1", "only one internal disk row is ever emitted");
    assert_eq!(field_list(&m, |e| e.label.as_str()), "nvme0n1,first,second,CARD", "an internal drive nothing mounted adds no row of its own");
    assert_eq!(m[0].size, Some(256060514304), "the machine row carries its device capacity");
    assert_eq!(format!("{},{}", m[1].label, m[2].label), "first,second", "both partitions of one stick are rows");
    assert_eq!(m[3].label, "CARD", "an unpartitioned removable disk is a row of its own");
    assert_eq!(m[3].device, "/dev/sdb", "an unpartitioned removable disk carries its own device node");
}

#[test]
fn label_ladder_falls_back_to_model_then_kernel_name() {
    let no_label = r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"disk","model":"USB Flash Disk","children":[{"name":"sda1","path":"/dev/sda1","label":null,"mountpoints":[null],"rm":true,"size":124656812032,"type":"part","model":null,"fstype":"vfat"}]}]}"#;
    assert_eq!(parse_devices(no_label, false)[0].label, "USB Flash Disk", "an unlabelled volume falls back to the drive's product name");
    let no_model = r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":true,"size":34359738368,"type":"disk","model":null,"children":[{"name":"sdb1","path":"/dev/sdb1","label":null,"mountpoints":[null],"rm":true,"size":34359738368,"type":"part","model":null,"fstype":"vfat"}]}]}"#;
    assert_eq!(parse_devices(no_model, false)[0].label, "sdb1", "a volume with neither label nor model falls back to the kernel name");
}

#[test]
fn labels_and_mountpoints_are_data_not_rewritten() {
    // A label is data off somebody else's filesystem, so the parser never rewrites it.
    let awkward = r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,"children":[{"name":"sda1","path":"/dev/sda1","label":"Sauvegarde & Co \"2026\" <b>","mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null,"fstype":"vfat"}]}]}"#;
    assert_eq!(parse_devices(awkward, false)[0].label, "Sauvegarde & Co \"2026\" <b>", "an awkward label survives the parse verbatim");
    let long_label = r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,"children":[{"name":"sda1","path":"/dev/sda1","label":"Photographs and scans of every receipt from two thousand and twenty six, quarter one through quarter four","mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null,"fstype":"vfat"}]}]}"#;
    assert_eq!(parse_devices(long_label, false)[0].label, "Photographs and scans of every receipt from two thousand and twenty six, quarter one through quarter four", "a very long label is elided by the row, never truncated by the parser");
    let spaced = r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,"children":[{"name":"sda1","path":"/dev/sda1","label":"USB Drive","mountpoints":["/run/media/gm/USB Drive"],"rm":true,"size":8589934592,"type":"part","model":null,"fstype":"vfat"}]}]}"#;
    assert_eq!(parse_devices(spaced, false)[0].path, "/run/media/gm/USB Drive", "a mountpoint with a space is opened verbatim");
    // A node with no name would build "/dev/undefined" and hand it to gio, so it is no row.
    let no_name = r#"{"blockdevices":[{"label":"nameless","mountpoints":[null],"rm":true,"size":8589934592,"type":"part","model":null}]}"#;
    assert_eq!(parse_devices(no_name, false).len(), 0, "a node with no name is not a row");
}

#[test]
fn lsblk_body_yields_no_network_mounts() {
    assert_eq!(parse_mounts(LIVE).len(), 0, "an lsblk body yields no network mounts");
}

#[test]
fn eject_verdict_is_read_from_a_fresh_listing() {
    let gone = r#"{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G"}]}"#;
    let media_out = r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":"0B","type":"disk","model":"USB Flash Disk"}]}"#;
    let swap = r#"{"blockdevices":[{"name":"sda","path":"/dev/sda","label":null,"mountpoints":[null],"rm":true,"size":8589934592,"type":"disk","model":null,"children":[{"name":"sda1","path":"/dev/sda1","label":null,"mountpoints":["[SWAP]"],"rm":true,"size":8589934592,"type":"part","model":null}]}]}"#;
    assert_eq!(verdict(LIVE, "/dev/sda1"), Verdict::Mounted, "a listing that still mounts the ejected volume refuses the verdict");
    assert_eq!(verdict(UNMOUNTED, "/dev/sda1"), Verdict::Safe, "a listing with the volume present but unmounted is safe");
    assert_eq!(verdict(gone, "/dev/sda1"), Verdict::Safe, "a listing the device has vanished from is safe");
    assert_eq!(verdict(media_out, "/dev/sda1"), Verdict::Safe, "a stick whose media ejected but whose disk node stayed is safe");
    assert_eq!(verdict(MANY, "/dev/sda2"), Verdict::Mounted, "a sibling partition still mounted refuses the verdict for its unmounted neighbour");
    assert_eq!(verdict(CRYPT, "/dev/sda1"), Verdict::Mounted, "a mounted crypt child under an unmounted partition refuses the verdict");
    assert_eq!(verdict(swap, "/dev/sda1"), Verdict::Mounted, "active swap on the stick counts as mounted");
    assert_eq!(verdict("not json at all\n", "/dev/sda1"), Verdict::Unknown, "garbage in place of a listing is unknown, never safe");
    assert_eq!(verdict("", "/dev/sda1"), Verdict::Unknown, "an empty body is unknown, never safe");
    assert_eq!(verdict("{}", "/dev/sda1"), Verdict::Unknown, "json with no blockdevices key is unknown, never safe");
    assert_eq!(verdict(r#"{"blockdevices":[]}"#, "/dev/sda1"), Verdict::Unknown, "a listing with no block devices at all is unknown, never safe");
    assert_eq!(verdict(LIVE, ""), Verdict::Unknown, "an empty device name is unknown, never safe");
}

#[test]
fn eject_sentences_name_the_verdict_and_never_say_safe_otherwise() {
    assert_eq!(sentence(Verdict::Safe, "128GB", &[], false).0.english(), "Ejected 128GB. It is safe to unplug.", "the safe verdict earns the unplug sentence");
    assert!(!sentence(Verdict::Safe, "128GB", &[], false).1, "the safe verdict is not an error");
    assert_eq!(sentence(Verdict::Safe, "128GB", &[], false).0.english(), "Ejected 128GB. It is safe to unplug.", "a real drive keeps the safe unplug sentence");
    assert_eq!(sentence(Verdict::Safe, "OMARCHY_202608", &[], true).0.english(), "Ejected OMARCHY_202608.", "a loop image's safe verdict says only that it was ejected");
    assert!(!sentence(Verdict::Safe, "ISO", &[], true).1, "a loop image's safe verdict is not an error");
    assert!(!sentence(Verdict::Safe, "ISO", &[], true).0.english().to_lowercase().contains("unplug"), "a loop image's successful eject never says unplug");
    assert_eq!(sentence(Verdict::Attached, "ISO", &[], true).0.english(), "Unmounted ISO, but its loop device is still attached.", "a failed loop detach keeps its exact sentence");
    let others: Vec<(Verdict, Vec<String>)> = vec![
        (Verdict::Attached, vec![]),
        (Verdict::Mounted, vec![]),
        (Verdict::Mounted, vec!["first".to_string()]),
        (Verdict::Mounted, vec!["first".to_string(), "second".to_string()]),
        (Verdict::Unknown, vec![]),
    ];
    for (v, list) in &others {
        assert_eq!(sentence(*v, "ISO", list, true), sentence(*v, "ISO", list, false), "loop identity preserves the {:?} verdict with {} blockers", v, list.len());
    }
    assert_eq!(sentence(Verdict::Mounted, "128GB", &[], false).0.english(), "128GB is still mounted; close what is using it.", "the mounted verdict with the row itself still mounted says try again");
    assert!(sentence(Verdict::Mounted, "128GB", &[], false).1, "the mounted verdict is an error");
    assert_eq!(sentence(Verdict::Mounted, "second", &["first".to_string()], false).0.english(), "second could not be ejected because first on the same drive is still mounted. Eject that instead.", "a sibling blocker is named instead of an instruction that would do nothing");
    let two = vec!["first".to_string(), "second".to_string()];
    assert_eq!(sentence(Verdict::Mounted, "third", &two, false).0.english(), "third could not be ejected because first, second on the same drive are still mounted. Eject those instead.", "two blockers are both named");
    assert_eq!(sentence(Verdict::Unknown, "128GB", &[], false).0.english(), "Could not confirm 128GB was ejected; do not unplug it yet.", "the unknown verdict says do not unplug");
    assert!(sentence(Verdict::Unknown, "128GB", &[], false).1, "the unknown verdict is an error");
    // The JS inputs "", undefined and "SAFE" cannot reach sentence: Verdict is an enum, so every value is checked below.
    let not_safe = [Verdict::Mounted, Verdict::Unknown, Verdict::Attached];
    let any_safe = not_safe.iter().any(|v| sentence(*v, "X", &["Y".to_string()], false).0.english().contains("safe to unplug"));
    assert!(!any_safe, "no verdict but safe ever says safe to unplug");
}

#[test]
fn blockers_name_what_still_holds_the_drive() {
    assert_eq!(blockers(LIVE, "/dev/sda1").join(","), "", "a row still mounted itself names no blocker, a re-press is the next step");
    assert_eq!(blockers(MANY, "/dev/sda2").join(","), "first", "an unmounted row names the mounted sibling by its label");
    assert_eq!(blockers(CRYPT, "/dev/sda1").join(","), "vault", "an unmounted partition names its mounted crypt child");
    let gone = r#"{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G"}]}"#;
    assert_eq!(blockers(gone, "/dev/sda1").join(","), "", "a vanished device names nothing");
    assert_eq!(blockers("not json", "/dev/sda1").join(","), "", "garbage names nothing");
}

#[test]
fn mount_wait_starts_only_when_gio_exits_zero() {
    assert!(mount_timer_start(0), "an exited gio starts the wait");
    assert!(!mount_timer_start(1), "a refused gio never starts it");
    assert!(!mount_timer_start(137), "a killed gio never starts it");
}

#[test]
fn power_off_queues_every_mounted_node_on_the_disk() {
    let queue = power_off_queue(VOLS, "/dev/sda", &[]);
    assert_eq!(queue.iter().map(|q| q.device.as_str()).collect::<Vec<_>>(), vec!["/dev/sda1", "/dev/sda2"], "both mounted volumes on the disk queue, the hidden ESP too");
    assert_eq!(queue[1].path, "/run/media/u/DATA", "a queued leg carries its mountpoint from the listing");
    assert_eq!(power_off_queue(VOLS, "/dev/sdb", &[]).iter().map(|q| q.device.as_str()).collect::<Vec<_>>(), vec!["/dev/sdb"], "the other disk queues only its own");
    // A null listing in the JS is the empty string here.
    assert_eq!(power_off_queue("", "/dev/sda", &[]).len(), 0, "no listing queues nothing");
}

#[test]
fn drives_holding_names_every_drive_under_a_volume() {
    let crypt = r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","children":[{"name":"sdb2","path":"/dev/sdb2","children":[{"name":"luks-x","path":"/dev/mapper/luks-x","type":"crypt"}]}]},{"name":"sdc","path":"/dev/sdc"}]}"#;
    assert_eq!(drives_holding(crypt, "/dev/mapper/luks-x"), vec!["/dev/sdb"], "an open crypt names the drive under its container");
    assert_eq!(drives_holding(crypt, "/dev/sdc"), vec!["/dev/sdc"], "a whole disk names itself");
    let spanned = r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","children":[{"name":"md0","path":"/dev/md0"}]},{"name":"sdc","path":"/dev/sdc","children":[{"name":"md0","path":"/dev/md0"}]}]}"#;
    assert_eq!(drives_holding(spanned, "/dev/md0"), vec!["/dev/sdb", "/dev/sdc"], "an array lsblk repeats under each member names both drives");
    assert!(drives_holding("not json", "/dev/sdb1").is_empty(), "an unparsable listing names no drive");
}

#[test]
fn power_off_falls_back_to_rail_volumes_when_the_listing_fails() {
    let rails = rail_volumes();
    assert_eq!(power_off_queue("not json", "/dev/sda", &rails), rail_queue(), "an unparsable listing falls back to mounted rail volumes on this disk");
    assert_eq!(power_off_queue(&VOLS[..VOLS.len() - 1], "/dev/sda", &rails), rail_queue(), "a truncated listing keeps the same rail fallback");
    assert_eq!(power_off_queue("", "/dev/sda", &rails), rail_queue(), "an empty listing keeps the same rail fallback");
    assert_eq!(power_off_queue(VOLS, "/dev/sda", &rails)[0].path, "/run/media/u/EFI", "a valid listing keeps hidden nodes and its own mountpoints instead of the fallback");
}

#[test]
fn power_off_dedupes_a_dm_node_repeated_under_two_partitions() {
    assert_eq!(power_off_queue(MAPPED, "/dev/sdc", &[]), vec![
        Queued { device: "/dev/mapper/vg-data".into(), path: "/mnt/data".into() },
        Queued { device: "/dev/mapper/vg-other".into(), path: "/mnt/other".into() },
    ], "a dm node repeated under two partitions queues once, distinct device paths both stay");
}

#[test]
fn sysfs_helpers_answer_block_name_and_written_sectors() {
    // The eject chain watches the disk's write counter, so a flushing leg restarts its deadline.
    assert_eq!(sys_base("/dev/sda"), "sda", "a disk path answers its sysfs block name");
    assert_eq!(sys_base("/dev/nvme0n1"), "nvme0n1", "an nvme disk answers whole");
    assert_eq!(sys_base(""), "", "a non-device answers nothing");
    assert_eq!(written_sectors("   1 0 2 3 0 0 42 0 0 0 0 0 0 0 0"), "42", "a stat read answers its written sectors");
    assert_eq!(written_sectors("1 2 3"), "", "a short read answers nothing");
}

#[test]
fn multiboot_windows_data_stays_visible_and_other_siblings_follow_the_switch() {
    let with = parse_devices(MULTIBOOT, true);
    let without = parse_devices(MULTIBOOT, false);
    assert_eq!(field_list(&with, |e| e.label.as_str()), "nvme0n1,Acer,Micron_2450,ryoku", "with the switch on, the system disk lists its real volumes beside the disk row");
    assert_eq!(if with.len() > 2 { with[2].label.as_str() } else { "" }, "Micron_2450", "an unlabeled sibling falls back to the drive's product name");
    assert_eq!(field_list(&without, |e| e.label.as_str()), "nvme0n1,Acer,Micron_2450", "usable Windows data volumes stay visible with the switch off");
    assert!(!(with.iter().any(|e| e.label == "ESP" || e.label == "OMARCHY_EFI")), "both ESPs stay out with the switch on");
    assert!(!(without.iter().any(|e| e.label == "ESP" || e.label == "OMARCHY_EFI")), "and they stay out with the switch off");
    assert!(!(with.iter().any(|e| e.device == "/dev/nvme0n1p2") || without.iter().any(|e| e.device == "/dev/nvme0n1p2")), "the reserved partition stays out on either switch value");
    assert!(!(with.iter().any(|e| e.label == "Recovery") || without.iter().any(|e| e.label == "Recovery")), "the Recovery partition stays out on either switch value");
    assert!(!(with.iter().any(|e| e.label == "RYOKUBOOT") || without.iter().any(|e| e.label == "RYOKUBOOT")), "RYOKUBOOT stays out on either switch value");
    assert!(!(with.iter().any(|e| e.path == "/" && e.kind == Kind::Volume)), "the root leaf itself is not a second row, the disk row above already opens it");
    assert!(!(without.iter().any(|e| e.path == "/" && e.kind == Kind::Volume)), "and it stays out with the switch off too");
    assert!(!(if with.len() > 1 { with[1].removable } else { true }), "a sibling volume is not offered an eject");
    assert_eq!(menu_ids(&with[1]), vec!["mountVolume"], "an unmounted sibling offers Mount");
}

#[test]
fn mounted_siblings_are_rows_only_off_the_system_mounts() {
    let win_disk = r#"{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":128060514304,"type":"part","model":null,"fstype":"btrfs","parttypename":"Linux filesystem","parttype":"0fc63daf-8483-4772-8e79-3d69d8477de4"},{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":"Win","mountpoints":["/mnt/win"],"rm":false,"size":128000000000,"type":"part","model":null,"fstype":"ntfs","parttypename":"Microsoft basic data","parttype":"ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"}]}]}"#;
    let home_disk = r#"{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":128060514304,"type":"part","model":null,"fstype":"btrfs","parttypename":"Linux filesystem","parttype":"0fc63daf-8483-4772-8e79-3d69d8477de4"},{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":"homevol","mountpoints":["/home"],"rm":false,"size":128000000000,"type":"part","model":null,"fstype":"ext4","parttypename":"Linux filesystem","parttype":"0fc63daf-8483-4772-8e79-3d69d8477de4"}]}]}"#;
    let win = parse_devices(win_disk, false);
    assert_eq!(field_list(&win, |e| e.label.as_str()), "nvme0n1,Win", "a sibling mounted at /mnt/win is a row with the switch off");
    let win_row = win.iter().find(|e| e.label == "Win");
    assert_eq!(win_row.map(menu_ids).unwrap_or_default(), vec!["open", "unmountVolume"], "a mounted sibling offers Open and Unmount");
    assert_eq!(parse_devices(home_disk, false).len(), 1, "a sibling mounted only at /home is no row with the switch off");
    assert_eq!(parse_devices(home_disk, true).len(), 1, "and it stays out with the switch on");
}
