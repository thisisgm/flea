//! Port of tests/js/mounts.js, second half: rail gate, eject marks, polls, mount errors, power, trash and kernel rows.
use super::eject::{release_action, releasable};
use super::menu::{rail_menu, MenuRow, RailAction};
use super::mounts::{gio_kernel_mounts, holding, poll_decision, rail_groups_ready, rail_key, same_entries, trashable, PollDecision};
use super::power::{mount_detail, mount_error, power_off_disk, Op};
use super::{Group, Kind, RailEntry};

/// The action ids a rail entry's menu offers, separators skipped.
fn ids(entry: &RailEntry) -> Vec<&'static str> {
    rail_menu(entry).iter().filter_map(MenuRow::action).map(|a| a.id()).collect()
}

/// The English sentence for a failed gio call, None when there is no message.
fn said(op: Op, exit: i32, stderr: &str, label: &str) -> Option<String> {
    mount_error(op, exit, stderr, label).map(|n| n.english())
}

#[test]
fn rail_groups_ready_waits_for_network_then_devices_until_deadline() {
    assert_eq!(rail_groups_ready(false, false, 0, 800), (false, false), "nothing answered before the deadline shows nothing");
    assert_eq!(rail_groups_ready(true, false, 0, 800), (true, false), "network answered alone shows network only");
    assert_eq!(rail_groups_ready(false, true, 0, 800), (false, false), "devices answered alone shows nothing, it would land above an empty network");
    assert_eq!(rail_groups_ready(true, true, 0, 800), (true, true), "both answered before the deadline shows both");
    assert_eq!(rail_groups_ready(false, false, 799, 800), (false, false), "one millisecond before the deadline still shows nothing");
    assert_eq!(rail_groups_ready(false, false, 800, 800), (true, true), "the deadline itself shows both whatever each has");
    assert_eq!(rail_groups_ready(true, false, 5000, 800), (true, true), "a late answer after the deadline changes nothing visible");
}

#[test]
fn releasable_draws_the_mark_on_mounted_removable_share_and_phone_rows_only() {
    let stick = RailEntry {
        label: "128GB".into(),
        group: Group::Device,
        kind: Kind::Volume,
        device: "/dev/sda1".into(),
        path: "/run/media/u/128GB".into(),
        mounted: true,
        removable: true,
        ..RailEntry::default()
    };
    let fixed_disk = RailEntry {
        label: "Vault".into(),
        group: Group::Device,
        kind: Kind::Volume,
        device: "/dev/sdb1".into(),
        path: "/mnt/vault".into(),
        mounted: true,
        ..RailEntry::default()
    };
    let smb = RailEntry { label: "NAS".into(), group: Group::Network, kind: Kind::Share, uri: "smb://h/data/".into(), mounted: true, ..RailEntry::default() };
    let nfs = RailEntry { label: "Archive".into(), group: Group::Network, kind: Kind::Share, uri: "nfs://h/archive/".into(), mounted: true, ..RailEntry::default() };
    let phone = RailEntry {
        label: "Pixel".into(),
        group: Group::Device,
        kind: Kind::Phone,
        uri: "mtp://x/".into(),
        path: "/run/user/1000/gvfs/x".into(),
        mounted: true,
        ..RailEntry::default()
    };
    let gphoto = RailEntry {
        label: "Camera".into(),
        group: Group::Device,
        kind: Kind::Phone,
        uri: "gphoto2://x/".into(),
        path: "/run/user/1000/gvfs/y".into(),
        mounted: true,
        ..RailEntry::default()
    };
    let afc = RailEntry {
        label: "iPhone".into(),
        group: Group::Device,
        kind: Kind::Phone,
        uri: "afc://x/".into(),
        path: "/run/user/1000/gvfs/z".into(),
        mounted: true,
        ..RailEntry::default()
    };
    let cloud = RailEntry {
        label: "gdrive".into(),
        group: Group::Network,
        kind: Kind::Cloud,
        path: "/home/u/gdrive".into(),
        mounted: true,
        ..RailEntry::default()
    };
    let idle = RailEntry { label: "128GB".into(), group: Group::Device, kind: Kind::Volume, device: "/dev/sda1".into(), removable: true, ..RailEntry::default() };
    let idle_phone = RailEntry { label: "Pixel".into(), group: Group::Device, kind: Kind::Phone, uri: "mtp://x/".into(), ..RailEntry::default() };
    let bookmark = RailEntry { label: "NAS".into(), group: Group::Network, kind: Kind::Share, uri: "smb://example.com/".into(), ..RailEntry::default() };
    assert!(releasable(&stick), "a mounted removable drive draws the mark");
    assert!(releasable(&smb), "a mounted SMB share draws the mark");
    assert!(!(releasable(&nfs)), "an NFS share keeps its square");
    assert!(releasable(&phone), "a mounted mtp phone draws the mark");
    assert!(releasable(&gphoto), "a mounted gphoto2 phone draws the mark");
    assert!(releasable(&afc), "a mounted afc phone draws the mark");
    assert!(!(releasable(&cloud)), "a cloud row keeps its square");
    assert!(!(releasable(&fixed_disk)), "a fixed internal volume keeps its square");
    assert!(!(releasable(&idle)), "an unmounted drive keeps its square");
    assert!(!(releasable(&idle_phone)), "an unmounted phone keeps its square");
    assert!(!(releasable(&bookmark)), "an unmounted share keeps its square");
    assert!(!(releasable(&RailEntry::default())), "no entry at all draws no mark");
}

#[test]
fn release_action_on_the_mark_matches_the_menu_release_row() {
    let stick = RailEntry {
        label: "128GB".into(),
        group: Group::Device,
        kind: Kind::Volume,
        device: "/dev/sda1".into(),
        path: "/run/media/u/128GB".into(),
        mounted: true,
        removable: true,
        ..RailEntry::default()
    };
    let smb = RailEntry { label: "NAS".into(), group: Group::Network, kind: Kind::Share, uri: "smb://h/data/".into(), mounted: true, ..RailEntry::default() };
    let nfs = RailEntry { label: "Archive".into(), group: Group::Network, kind: Kind::Share, uri: "nfs://h/archive/".into(), mounted: true, ..RailEntry::default() };
    let phone = RailEntry {
        label: "Pixel".into(),
        group: Group::Device,
        kind: Kind::Phone,
        uri: "mtp://x/".into(),
        path: "/run/user/1000/gvfs/x".into(),
        mounted: true,
        ..RailEntry::default()
    };
    let gphoto = RailEntry {
        label: "Camera".into(),
        group: Group::Device,
        kind: Kind::Phone,
        uri: "gphoto2://x/".into(),
        path: "/run/user/1000/gvfs/y".into(),
        mounted: true,
        ..RailEntry::default()
    };
    let afc = RailEntry {
        label: "iPhone".into(),
        group: Group::Device,
        kind: Kind::Phone,
        uri: "afc://x/".into(),
        path: "/run/user/1000/gvfs/z".into(),
        mounted: true,
        ..RailEntry::default()
    };
    let cloud = RailEntry {
        label: "gdrive".into(),
        group: Group::Network,
        kind: Kind::Cloud,
        path: "/home/u/gdrive".into(),
        mounted: true,
        ..RailEntry::default()
    };
    assert_eq!(release_action(&stick), Some(RailAction::Eject), "the mark on a drive is Eject");
    assert_eq!(release_action(&smb), Some(RailAction::Unmount), "the mark on a share is Unmount");
    assert_eq!(release_action(&phone), Some(RailAction::UnmountPhone), "the mark on a phone is its Unmount");
    assert_eq!(release_action(&gphoto), Some(RailAction::UnmountPhone), "the mark on a gphoto2 phone is its Unmount");
    assert_eq!(release_action(&afc), Some(RailAction::UnmountPhone), "the mark on an afc phone is its Unmount");
    assert_eq!(release_action(&nfs), None, "a row with no mark has no click action");
    assert_eq!(release_action(&cloud), None, "and neither does a cloud row");
}

#[test]
fn poll_decision_rebuilds_only_on_changed_text() {
    assert_eq!(poll_decision(false, "a", "", "b", ""), PollDecision::None, "before the first listing nothing rebuilds");
    assert_eq!(poll_decision(true, "a", "a", "b", "b"), PollDecision::Skip, "unchanged texts rebuild nothing");
    assert_eq!(poll_decision(true, "a", "x", "b", "b"), PollDecision::Rebuild, "a changed listing rebuilds");
    assert_eq!(poll_decision(true, "a", "a", "b", "y"), PollDecision::Rebuild, "a changed mountinfo rebuilds");
    assert_eq!(poll_decision(true, "a", "x", "b", "y"), PollDecision::Rebuild, "both changed still rebuilds once");
}

#[test]
fn mount_error_names_the_failure_in_one_plain_sentence() {
    assert_eq!(said(Op::Mount, 1, "gio: Error 1: target is busy", "128GB"), Some("128GB is busy; close what is using it and try again.".to_string()), "a busy target names what holds it");
    assert_eq!(said(Op::Mount, 1, "Not authorized to perform operation", "Vault"), Some("Vault was not mounted: you do not have permission.".to_string()), "a polkit refusal names the refusal");
    assert_eq!(said(Op::Mount, 1, "No volume for device /dev/sda9", "sda9"), Some("sda9 cannot be mounted on this computer.".to_string()), "no volume for the id names the missing volume");
    assert_eq!(said(Op::Mount, 1, "Location is already mounted", "isos"), None, "already mounted opens rather than failing");
    assert_eq!(said(Op::Mount, 1, "No medium found", "CARD"), Some("CARD has no disc or card in it.".to_string()), "no medium names the empty drive");
    assert_eq!(said(Op::Mount, 1, "weird new failure", "128GB"), Some("128GB could not be mounted.".to_string()), "an unknown error falls back to the plain sentence");
    assert_eq!(said(Op::Unmount, 1, "Not authorized", "Vault"), Some("Vault was not unmounted: you do not have permission.".to_string()), "a refused unmount names the refusal too");
    assert_eq!(said(Op::Eject, 1, "Not authorized", "Disk"), Some("Disk was not ejected: you do not have permission.".to_string()), "a refused eject names the eject");
    assert_eq!(said(Op::Mount, 0, "target is busy", "128GB"), None, "exit 0 never reads stderr");
}

#[test]
fn mount_error_names_a_damaged_file_system() {
    let damaged = "Error mounting /dev/sdb1 at /run/media/user/DATA #2: wrong fs type, bad option, bad superblock on /dev/sdb1, missing codepage or helper program, or other error";
    let damaged_line = "DATA #2 could not be mounted: its file system is damaged or not supported.";
    assert_eq!(said(Op::Mount, 1, damaged, "DATA #2"), Some(damaged_line.to_string()), "a bad superblock names the damaged file system");
    assert_eq!(said(Op::Mount, 1, "gio: Error mounting /dev/sdc1 at /run/media/g/X: wrong fs type", "X"), Some("X could not be mounted: its file system is damaged or not supported.".to_string()), "wrong fs type alone is the damaged sentence");
    assert_eq!(said(Op::Mount, 1, "mount: unknown filesystem type 'zfs_member'", "Pool"), Some("Pool could not be mounted: its file system is damaged or not supported.".to_string()), "an unknown filesystem type is the damaged sentence");
    assert_eq!(said(Op::Mount, 1, "can't read superblock on /dev/sdb1", "Disk"), Some("Disk could not be mounted: its file system is damaged or not supported.".to_string()), "an unreadable superblock is the damaged sentence");
    assert_eq!(said(Op::Mount, 1, "mount(2) system call failed: Structure needs cleaning.", "Disk"), Some("Disk could not be mounted: its file system is damaged or not supported.".to_string()), "a structure that needs cleaning is the damaged sentence");
    assert_eq!(said(Op::Unmount, 1, damaged, "Disk"), Some("Disk could not be unmounted.".to_string()), "an unmount never takes the mount sentence");
}

#[test]
fn mount_detail_keeps_only_the_reason_gio_gave() {
    let damaged = "Error mounting /dev/sdb1 at /run/media/user/DATA #2: wrong fs type, bad option, bad superblock on /dev/sdb1, missing codepage or helper program, or other error";
    let reason = "wrong fs type, bad option, bad superblock on /dev/sdb1, missing codepage or helper program, or other error";
    assert_eq!(mount_detail(damaged), reason, "the detail drops gio's lead and keeps the reason verbatim");
    assert_eq!(mount_detail(&format!("gio: {damaged}\n")), reason, "the detail drops a gio: prefix and the lead");
    assert_eq!(mount_detail("gio: Error 1: target is busy"), "Error 1: target is busy", "a reason with no lead keeps itself");
    assert_eq!(mount_detail("Not authorized\n  to perform operation\n"), "Not authorized to perform operation", "a multi-line reason is one line");
    assert_eq!(mount_detail("Error unmounting /dev/sdb1: target is busy"), "target is busy", "an unmount reason drops its lead");
    assert_eq!(mount_detail("gio: Error ejecting /dev/sdb: Not authorized\n"), "Not authorized", "an eject reason drops its lead");
    assert_eq!(mount_detail("gio: file:///run/media/u/Vault: Error unmounting /dev/sdb1: target is busy"), "target is busy", "an unmount reason drops gio's file-URI lead");
    assert_eq!(mount_detail("gio: file:///run/media/u/Stick: Error ejecting /dev/sdb: Not authorized\n"), "Not authorized", "an eject reason drops gio's file-URI lead");
    assert_eq!(mount_detail("gio: file:///nonexistent: Containing mount for file /nonexistent not found"), "Containing mount for file /nonexistent not found", "a lookup failure drops gio's file-URI lead");
    assert_eq!(mount_detail("Not authorized"), "Not authorized", "an unmount refusal with no lead keeps itself");
    assert_eq!(mount_detail(""), "", "an empty stderr has no detail");
    // A null stderr has no Rust form; the empty string is its counterpart.
    assert_eq!(mount_detail(""), "", "a missing stderr has no detail");
}

#[test]
fn power_off_disk_maps_a_partition_to_its_disk() {
    assert_eq!(power_off_disk("/dev/sda1"), "/dev/sda", "a scsi partition powers off its disk");
    assert_eq!(power_off_disk("/dev/nvme0n1p2"), "/dev/nvme0n1", "an nvme namespace strip powers off its disk");
    assert_eq!(power_off_disk("/dev/mmcblk0p1"), "/dev/mmcblk0", "an mmc partition powers off its card");
    assert_eq!(power_off_disk("/dev/mapper/luks-vault"), "", "a mapper leaf has no disk to power off");
    assert_eq!(power_off_disk(""), "", "an empty device has no disk to power off");
}

#[test]
fn trashable_is_false_for_gvfs_and_read_only_folders() {
    assert!(trashable("/home/gm/work", Some(true)), "a writable local folder trashes");
    assert!(!(trashable("/run/user/1000/gvfs/smb-share:server=h,share=d/x", Some(true))), "a gvfs share never trashes");
    assert!(!trashable("/home/gm/work", Some(false)), "a read-only folder never trashes");
    assert!(trashable("/home/gm/work", None), "an old caller passing no writability still trashes a local folder");
}

#[test]
fn gio_kernel_mounts_decodes_each_file_line() {
    let kgio = concat!(
        "Mount(0): photos -> file:///mnt/kmount-photos\n",
        "Mount(1): iso-images -> file:///mnt/kmount-iso/\n",
        "Mount(2): isos on 192.168.1.10 -> smb://192.168.1.10/isos/\n",
        "Mount(3): 32GB USB Drive -> file:///run/media/gm/32GB%20USB%20Drive/\n",
        "Mount(4): idle-share -> file:///mnt/kmount-idle\n",
    );
    let krows = gio_kernel_mounts(kgio);
    assert_eq!(krows.len(), 4, "every file:// line decodes with gio's own spelling");
    assert_eq!(krows[0].label, "photos", "the nfs line keeps gio's mount name");
    assert_eq!(krows[0].uri, "file:///mnt/kmount-photos", "the nfs line keeps gio's uri spelling");
    assert_eq!(krows[0].path, "/mnt/kmount-photos", "the nfs line resolves to its mountpoint");
    assert_eq!(krows[1].path, "/mnt/kmount-iso", "a trailing slash still resolves to the mountpoint");
    assert_eq!(krows[2].path, "/run/media/gm/32GB USB Drive", "a percent-encoded path decodes before the compare");
    assert_eq!(gio_kernel_mounts("").len(), 0, "no gio text means nothing to join");
    assert_eq!(gio_kernel_mounts("not gio output\n").len(), 0, "garbage means nothing to join");
    assert_eq!(gio_kernel_mounts("Mount(0): isos -> smb://h/isos/\n").len(), 0, "a non-file line is not a kernel enrichment");
}

#[test]
fn kernel_rows_ride_the_release_path_with_marks_split_by_fstype() {
    let knfs = RailEntry {
        path: "/mnt/kmount-photos".into(),
        label: "photos".into(),
        group: Group::Network,
        kind: Kind::Share,
        uri: "file:///mnt/kmount-photos".into(),
        fstype: "nfs4".into(),
        mounted: true,
        glyph: "server".into(),
        editable: Some(false),
        ..RailEntry::default()
    };
    let kcifs = RailEntry {
        path: "/mnt/kmount-iso".into(),
        label: "iso-images".into(),
        group: Group::Network,
        kind: Kind::Share,
        uri: "file:///mnt/kmount-iso".into(),
        fstype: "cifs".into(),
        mounted: true,
        glyph: "server".into(),
        editable: Some(false),
        ..RailEntry::default()
    };
    let kbulk = RailEntry {
        path: "/mnt/kmount-bulk".into(),
        label: "kmount-bulk".into(),
        group: Group::Network,
        kind: Kind::Share,
        uri: "/mnt/kmount-bulk".into(),
        fstype: "nfs".into(),
        mounted: true,
        glyph: "server".into(),
        editable: Some(false),
        ..RailEntry::default()
    };
    let krclone = RailEntry {
        path: "/mnt/kmount-rclone".into(),
        label: "kmount-rclone".into(),
        group: Group::Network,
        kind: Kind::Share,
        uri: "/mnt/kmount-rclone".into(),
        fstype: "fuse.rclone".into(),
        mounted: true,
        glyph: "server".into(),
        editable: Some(false),
        ..RailEntry::default()
    };
    assert_eq!(ids(&kcifs), vec!["open", "unmount"], "a kernel cifs row opens beside its unmount and nothing else");
    assert_eq!(ids(&kcifs), vec!["open", "unmount"], "its rows take the same actions a share row's do");
    assert_eq!(ids(&knfs), vec!["open", "unmount"], "a kernel nfs row keeps the same two rows");
    assert!(releasable(&kcifs), "a kernel cifs mark releases like a gvfs smb one");
    assert!(!(releasable(&knfs)), "a kernel nfs row keeps its square like a gvfs nfs one");
    assert!(!(releasable(&kbulk)), "a kernel row gio never lists keeps its square too");
    assert_eq!(release_action(&kcifs), Some(RailAction::Unmount), "the mark on a kernel cifs row is Unmount");
    assert_eq!(release_action(&knfs), None, "a kernel nfs row has no mark to click");
    assert_eq!(rail_key(&kcifs), "file:///mnt/kmount-iso", "a kernel row's key is its file uri");
    assert_eq!(rail_key(&kbulk), "/mnt/kmount-bulk", "a row gio never lists is keyed by its path");
    assert_eq!(ids(&kbulk), vec!["open", "unmount"], "a row gio never lists opens beside its unmount and nothing else");
    assert_eq!(ids(&krclone), vec!["open", "unmount"], "a FUSE network row opens beside its unmount too");
    assert!(!(releasable(&krclone)), "a FUSE network row keeps its square");
    let rail = [knfs.clone(), kcifs.clone()];
    assert_eq!(holding(&rail, "/mnt/kmount-iso/films"), Some(&kcifs), "a listing inside a kernel mount resolves to its row");
    assert_eq!(holding(&rail, "/mnt/kmount-iso"), Some(&kcifs), "the mountpoint itself resolves too");
    assert_eq!(holding(&rail, "/mnt/kmount-iso-old"), None, "a sibling prefix does not resolve");
    let remounted = RailEntry { fstype: "nfs".into(), ..kcifs.clone() };
    assert!(!same_entries(std::slice::from_ref(&kcifs), &[remounted]), "an fstype change rebuilds the row");
    assert!(same_entries(std::slice::from_ref(&kcifs), std::slice::from_ref(&kcifs)), "the same fstype compares equal");
    // NOT PORTED: idle automount no row, triggered one nfs4: calls Cloud.parseNetworkMounts, owned by the network unit.
}
