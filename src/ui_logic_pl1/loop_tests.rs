//! Ported from tests/js/loopdevices.js: a loop-mounted .iso rail row and the eject command that releases it.
use super::devices::parse_devices;
use super::power::{eject_command, DETACH_FAILED_EXIT};
use super::RailEntry;

// A .iso opened from the file list, captured live from lsblk with the mount path made generic.
const MOUNTED_ISO: &str = concat!(
    r#"{"blockdevices":["#,
    r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
    r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
    r#"{"name":"loop0","path":"/dev/loop0","label":null,"mountpoints":[null],"rm":false,"size":6224838656,"type":"loop","model":null,"#,
    r#""children":[{"name":"loop0p1","path":"/dev/loop0p1","label":"OMARCHY_202608","mountpoints":["/run/media/user/OMARCHY_202608"],"rm":false,"size":6224838656,"type":"part","model":null},"#,
    r#"{"name":"loop0p2","path":"/dev/loop0p2","label":null,"mountpoints":[null],"rm":false,"size":24117248,"type":"part","model":null}]}"#,
    r#"]}"#,
);

// The same .iso with its loop device carrying a filesystem label, fstype and mountpoint of its own.
const HYBRID: &str = concat!(
    r#"{"blockdevices":["#,
    r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
    r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
    r#"{"name":"loop0","path":"/dev/loop0","label":"WHOLE_ISO","mountpoints":["/run/media/user/WHOLE_ISO"],"rm":false,"size":6224838656,"type":"loop","model":null,"fstype":"iso9660","#,
    r#""children":[{"name":"loop0p1","path":"/dev/loop0p1","label":"OMARCHY_202608","mountpoints":["/run/media/user/OMARCHY_202608"],"rm":false,"size":6224838656,"type":"part","model":null},"#,
    r#"{"name":"loop0p2","path":"/dev/loop0p2","label":null,"mountpoints":[null],"rm":false,"size":24117248,"type":"part","model":null}]}"#,
    r#"]}"#,
);

// A loop device with nothing mounted on it, systemd or a package manager may hold one.
const BARE_LOOP: &str = concat!(
    r#"{"blockdevices":["#,
    r#"{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
    r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
    r#"{"name":"loop0","path":"/dev/loop0","label":"FLEATEST","mountpoints":[null],"rm":false,"size":67108864,"type":"loop","model":null}"#,
    r#"]}"#,
);

#[test]
fn mounted_loop_partition_is_a_device_row_marked_loop() {
    let iso = parse_devices(MOUNTED_ISO, false);
    assert_eq!(iso.len(), 2, "the mounted loop partition is a device row despite rm being false");
    assert_eq!(iso[1].label, "OMARCHY_202608", "the mounted loop partition takes the filesystem label");
    assert_eq!(iso[1].device, "/dev/loop0p1", "the mounted loop partition carries its device node for gio");
    assert_eq!(iso[1].path, "/run/media/user/OMARCHY_202608", "the mounted loop partition carries the mountpoint lsblk reported");
    assert!(iso[1].mounted, "the mounted loop partition reads as mounted");
    assert_eq!(iso.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(), vec!["nvme0n1", "OMARCHY_202608"], "the loop device's own row is not duplicated");
    assert!(!(iso.iter().any(|e| e.device == "/dev/loop0p2")), "the loop device's unmounted second partition is not a row");
    assert!(!iso[1].removable, "a mounted loop partition is not marked removable, since rm never goes true for one");
    // Eject routes a loop row to gio mount -u, because -e hangs on a loop volume.
    assert!(iso[1].loop_device, "the mounted loop partition is marked loop for eject to route around");
}

#[test]
fn loop_eject_runs_a_shell_with_argument_only_paths() {
    let iso = parse_devices(MOUNTED_ISO, false);
    let loop_eject = eject_command(&iso[1]);
    assert_eq!(loop_eject[..2].join(" "), "sh -c", "a loop row's eject uses a shell with argument-only paths");
    assert_eq!(loop_eject[6], "/sys/block/loop0/loop/backing_file", "a loop row passes its validated backing-file path as argument 3");
    assert_eq!(loop_eject[7], "/sys/block/loop0/loop/autoclear", "a loop row passes its validated autoclear path as argument 4");
    assert_eq!(DETACH_FAILED_EXIT, 3, "a failed detach exits with its own code, apart from a failed unmount");
    assert_eq!(loop_eject[3..6].join(" "), "sh /run/media/user/OMARCHY_202608 /dev/loop0", "a loop row's eject passes the mountpoint and the loop device as arguments");
}

#[test]
fn stick_and_unnumbered_loop_ejects_take_their_own_route() {
    let stick = RailEntry { device: "/dev/sda1".into(), path: "/run/media/user/128GB".into(), loop_device: false, ..RailEntry::default() };
    assert_eq!(eject_command(&stick).join(" "), "gio mount -e /run/media/user/128GB", "a stick's eject stays gio mount -e on its mountpoint");
    let unnumbered = RailEntry { device: "/dev/mapper/x".into(), path: "/m".into(), loop_device: true, ..RailEntry::default() };
    assert_eq!(eject_command(&unnumbered).join(" "), "gio mount -u /m", "a loop row whose device is no loop node falls back to -u alone");
}

#[test]
fn eject_keeps_the_exact_loop_number_for_every_partition_suffix() {
    for number in [0, 9, 10, 12, 1234] {
        for suffix in ["", "p1", "p12"] {
            let device = format!("/dev/loop{number}");
            let entry = RailEntry { device: format!("{device}{suffix}"), path: "/m".into(), loop_device: true, ..RailEntry::default() };
            assert_eq!(
                eject_command(&entry)[5..].join("|"),
                format!("{device}|/sys/block/loop{number}/loop/backing_file|/sys/block/loop{number}/loop/autoclear"),
                "eject keeps the exact loop number for {device}{suffix}"
            );
        }
    }
}

#[test]
fn mounted_hybrid_loop_parent_is_a_row_beside_its_child() {
    let parents = parse_devices(HYBRID, false);
    let parent = parents.iter().find(|e| e.device == "/dev/loop0");
    assert_eq!(parents.len(), 3, "a mounted hybrid loop parent is a row beside its mounted child");
    assert_eq!(parent.map(|e| e.path.as_str()).unwrap_or("missing"), "/run/media/user/WHOLE_ISO", "the mounted hybrid loop parent keeps its mountpoint");
    let route = parent.map(|e| eject_command(e)[4..6].join("|")).unwrap_or_else(|| "missing".to_string());
    assert_eq!(route, "/run/media/user/WHOLE_ISO|/dev/loop0", "the mounted hybrid loop parent keeps its loop eject route");
    assert!(parents.iter().any(|e| e.device == "/dev/loop0p1"), "walking a mounted parent still finds its mounted child");
}

// The six ejectMessage cases (exit 3, 1 and 0 against a gone, kept or sibling mount) run against the GUI Devices machine: crates/flea-gui/src/g2/pl1/eject_tests.rs.

#[test]
fn unmounted_loop_partition_with_a_filesystem_is_not_a_row() {
    // With unmounted volumes shown, only the partition gio mounted is a row.
    let with_data = MOUNTED_ISO.replace(
        r#""name":"loop0p2","path":"/dev/loop0p2","label":null,"#,
        r#""name":"loop0p2","path":"/dev/loop0p2","label":"DATA","fstype":"ext4","#,
    );
    let shown = parse_devices(&with_data, true);
    assert_eq!(shown.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(), vec!["nvme0n1", "OMARCHY_202608"], "an unmounted loop partition with a filesystem is not a row with unmounted shown");
}

#[test]
fn unmounted_bare_loop_device_is_not_a_device_row() {
    assert_eq!(parse_devices(BARE_LOOP, false).len(), 1, "an unmounted bare loop device is still not a device row");
}
