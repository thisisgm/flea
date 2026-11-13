//! Unlock card sentences, udisksctl output parsing, and the locked rows lsblk yields for a LUKS container.
use super::devices::parse_devices;
use super::unlock::{
    crypto_name, locked_row, mount_note, lock_note, naming_row, parse_dbus_error, parse_mounted, parse_unlocked, refusal_note,
    subtitle, title, unlock_argv, unlock_failed_note, unlocked_note, unlocking_note, wrong_note,
};
use super::RailEntry;

#[test]
#[rustfmt::skip]
fn card_sentences_name_the_drive() {
    assert_eq!(title("Backup").english(), "Unlock Backup", "the dialog titles itself for the drive");
    assert_eq!(unlocking_note("Backup").english(), "Unlocking Backup…", "unlocking names the drive with an ellipsis");
    // Authorized change: wrongNote no longer names the drive; it reads the new text with no label.
    assert_eq!(wrong_note().english(), "That passphrase did not work.", "a wrong passphrase names the drive");
    assert_eq!(unlocked_note("Backup").english(), "Unlocked and mounted Backup.", "success names the drive");
    assert_eq!(lock_note("Backup").english(), "Backup was unmounted but could not be locked; close what is using it.", "a lock that fails names the drive");
    assert_eq!(mount_note("Backup").english(), "Backup was unlocked but could not be mounted.", "an unlock without a mount names the drive");
    assert_eq!(unlock_failed_note("Backup").english(), "Backup could not be unlocked.", "a refused unlock names the drive");
}

#[test]
#[rustfmt::skip]
fn parse_unlock_and_mount_output() {
    assert_eq!(parse_unlocked("Unlocked /dev/sda1 as /dev/dm-0."), "/dev/dm-0", "the mapped device is read off the unlock line");
    assert_eq!(parse_mounted("Mounted /dev/dm-0 at /run/media/gm/Backup."), "/run/media/gm/Backup", "the mountpoint is read off the mount line");
    assert_eq!(parse_mounted("Mounted /dev/dm-0 at /run/media/gm/My Backup."), "/run/media/gm/My Backup", "a label with a space keeps its mountpoint");
}

#[test]
#[rustfmt::skip]
fn subtitle_joins_the_parts_present() {
    assert_eq!(subtitle("Backup", "500.0 GB", "LUKS2").english(), "Backup · 500.0 GB · LUKS2", "the name row joins all three parts");
    assert_eq!(subtitle("Backup", "", "LUKS2").english(), "Backup · LUKS2", "the name row without a size joins two");
    assert_eq!(subtitle("Backup", "", "").english(), "Backup", "the name row alone is the label");
}

#[test]
#[rustfmt::skip]
fn crypto_name_reads_the_luks_version() {
    assert_eq!(crypto_name("crypto_LUKS", "2"), "LUKS2", "a LUKS2 container names its version");
    assert_eq!(crypto_name("crypto_LUKS", "1"), "LUKS1", "a LUKS1 container names its version");
    assert_eq!(crypto_name("crypto_LUKS", ""), "LUKS", "a LUKS container without a version reads as LUKS");
    assert_eq!(crypto_name("crypto_plain", "2"), "PLAIN", "another crypto type keeps its name");
}

#[test]
#[rustfmt::skip]
fn dbus_error_name_is_read_off_the_daemon_line() {
    assert_eq!(parse_dbus_error("Error unlocking /dev/sdb1: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: x"), "org.freedesktop.UDisks2.Error.Failed", "the D-Bus error name is read off the daemon line");
    assert_eq!(parse_dbus_error("Fehler beim Entsperren: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: y"), "org.freedesktop.UDisks2.Error.Failed", "translated prose around the name changes nothing");
    assert_eq!(parse_dbus_error("GDBus.Error:org.freedesktop.UDisks2.Error.NotAuthorized: z"), "org.freedesktop.UDisks2.Error.NotAuthorized", "another error name is answered as itself");
    assert_eq!(parse_dbus_error("boom"), "", "output without a name answers empty");
}

#[test]
#[rustfmt::skip]
fn unlock_argv_takes_the_key_from_stdin() {
    assert_eq!(unlock_argv("/dev/sdb1").join(" "), "udisksctl unlock --block-device /dev/sdb1 --key-file /dev/stdin", "the key reaches udisksctl through /dev/stdin, since it opens --key-file as a path");
}

#[test]
#[rustfmt::skip]
fn only_the_daemon_failed_error_reads_as_a_wrong_passphrase() {
    assert_eq!(refusal_note("Error unlocking /dev/sda1: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: Incorrect passphrase.", "Backup").english(), "That passphrase did not work.", "only the daemon's Failed reads as a wrong passphrase");
    assert_eq!(refusal_note("Error unlocking /dev/sda1: Failed to open file \u{201c}-\u{201d}: No such file or directory", "Backup").english(), "Backup could not be unlocked.", "a local failure with no daemon error is never called a wrong passphrase");
    assert_eq!(refusal_note("GDBus.Error:org.freedesktop.UDisks2.Error.NotAuthorized: z", "Backup").english(), "Backup could not be unlocked.", "a refused authorization could not unlock");
}

#[test]
#[rustfmt::skip]
fn locked_container_on_a_stick_is_a_rail_row() {
    let locked = [
        r#"{"blockdevices":[{"name":"nvme0n1","path":"/dev/nvme0n1","label":null,"mountpoints":[null],"rm":false,"size":256060514304,"type":"disk","model":"KBG40ZNS256G","#,
        r#""children":[{"name":"nvme0n1p1","path":"/dev/nvme0n1p1","label":null,"mountpoints":["/"],"rm":false,"size":256060514304,"type":"part","model":null}]},"#,
        r#"{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":true,"size":500000000000,"type":"disk","model":"Vault Drive","#,
        r#""children":[{"name":"sdb1","path":"/dev/sdb1","label":"Backup","mountpoints":[null],"rm":true,"size":500000000000,"type":"part","fstype":"crypto_LUKS","fsver":"2","model":null}]}"#,
        r#"]}"#,
    ]
    .concat();
    let rows = parse_devices(&locked, false);
    assert_eq!(rows.len(), 2, "a locked container on a stick is a rail row");
    assert_eq!(rows[1].label, "Backup", "the locked row carries its label");
    assert_eq!(rows[1].device, "/dev/sdb1", "the locked row carries its device node");
    assert!(rows[1].locked, "the locked row reads as locked");
    assert_eq!(rows[1].fsver, "2", "the locked row carries lsblk's FSVER");
    assert!(!rows[1].mounted, "the locked row is not mounted");
}

#[test]
#[rustfmt::skip]
fn shimmed_loop_container_is_one_locked_row() {
    let loop_locked = [
        r#"{"blockdevices":[{"name":"%N","path":"/dev/loop0","label":null,"mountpoints":[null],"rm":%R,"#,
        r#""tran":null,"size":33554432,"type":"loop","model":null,"fstype":"crypto_LUKS"}]}"#,
    ]
    .concat();
    let rows = parse_devices(&loop_locked.replace("%N", "cap-loop0").replace("%R", "true"), false);
    assert!(rows.len() == 1 && rows[0].locked, "the shimmed loop container is one locked row");
    assert_eq!(rows[0].device, "/dev/loop0", "the shimmed loop row keeps the real device node");
    assert_eq!(rows[0].label, "cap-loop0", "the shimmed loop row is labelled by its shimmed name");
    assert_eq!(parse_devices(&loop_locked.replace("%N", "loop0").replace("%R", "true"), false).len(), 0, "a loop name stays no row even when removable");
}

#[test]
#[rustfmt::skip]
fn unlocked_named_container_keeps_the_locked_rows_name() {
    let named = [
        r#"{"blockdevices":[{"name":"sdb","path":"/dev/sdb","label":null,"mountpoints":[null],"rm":true,"size":500000000000,"type":"disk","model":"Vault Drive","#,
        r#""children":[{"name":"sdb1","path":"/dev/sdb1","label":"Backup","mountpoints":[null],"rm":true,"size":500000000000,"type":"part","fstype":"crypto_LUKS","#,
        r#""children":[{"name":"luks-1","path":"/dev/mapper/luks-1","label":"S1LUKS","mountpoints":[%M],"rm":false,"size":499983810560,"type":"crypt","fstype":"ext4"}]}]}]}"#,
    ]
    .concat();
    let unlocked = parse_devices(&named.replace("%M", "\"/run/media/gm/S1LUKS\""), false);
    assert_eq!(
        unlocked.iter().map(|r| format!("{}|{}|{}", r.label, r.path, r.locked)).collect::<Vec<_>>().join(","),
        "Backup|/run/media/gm/S1LUKS|false",
        "an unlocked named container keeps the locked row's name and opens its own folder"
    );
    assert_eq!(
        parse_devices(&named.replace("%M", "null"), true).iter().map(|r| r.label.clone()).collect::<Vec<_>>().join(","),
        "Backup",
        "mid-unlock, before the mount, the row already reads the container's name"
    );
    let unnamed = named.replace("\"label\":\"Backup\"", "\"label\":null").replace("%M", "\"/run/media/gm/S1LUKS\"");
    assert_eq!(parse_devices(&unnamed, false)[0].label, "S1LUKS", "an unnamed container's unlocked volume reads its own label, as it did");
}

#[test]
#[rustfmt::skip]
fn card_name_comes_from_the_locked_row_for_the_device() {
    let rail = [
        RailEntry { label: "Backup".into(), device: "/dev/sdb1".into(), locked: false, size: Some(1), ..RailEntry::default() },
        RailEntry { label: "Backup".into(), device: "/dev/loop0".into(), locked: true, size: Some(33554432), fstype: "crypto_LUKS".into(), ..RailEntry::default() },
    ];
    assert_eq!(locked_row(&rail, "/dev/loop0").map(|r| r.size), Some(Some(33554432)), "the CLI device finds its locked row");
    assert!(locked_row(&rail[..1], "/dev/sdb1").is_none(), "an unlocked row of the same device is never the card's name");
    assert!(locked_row(&[], "/dev/loop0").is_none(), "a rail that has not listed the device yet names nothing");
    // The host naming the CLI card on a rail rebuild is crates/flea-gui/src/g2/device_rail/host_tests.rs.
}

#[test]
#[rustfmt::skip]
fn hidden_rail_finds_the_locked_row_in_the_fallback_list() {
    let fallback = [RailEntry { label: "cap-loop0".into(), device: "/dev/loop0".into(), locked: true, size: Some(33554432), fstype: "crypto_LUKS".into(), fsver: "2".into(), ..RailEntry::default() }];
    let hidden_row = naming_row(&[], &fallback, "/dev/loop0");
    assert_eq!(hidden_row.map(|r| r.label.as_str()), Some("cap-loop0"), "a hidden rail finds the locked row in the fallback list");
    assert_eq!(hidden_row.map(|r| r.size), Some(Some(33554432)), "the hidden-rail row carries its size");
    assert_eq!(hidden_row.map(|r| crypto_name(&r.fstype, &r.fsver)), Some("LUKS2".to_string()), "the hidden-rail row reads its LUKS version");
    let rail_row = [RailEntry { label: "Backup".into(), device: "/dev/sdb1".into(), locked: true, size: Some(1), fstype: "crypto_LUKS".into(), fsver: "2".into(), ..RailEntry::default() }];
    assert_eq!(naming_row(&rail_row, &fallback, "/dev/sdb1").map(|r| r.label.as_str()), Some("Backup"), "the rail row still wins when both lists name the device");
}

#[test]
#[rustfmt::skip]
fn bitlocker_partition_is_a_locked_row() {
    let bitlocker = [
        r#"{"blockdevices":[{"name":"nvme1n1","path":"/dev/nvme1n1","label":null,"mountpoints":[null],"rm":false,"size":4000787030016,"type":"disk","model":"Samsung SSD 990 PRO 4TB","#,
        r#""children":[{"name":"nvme1n1p3","path":"/dev/nvme1n1p3","label":"Windows","mountpoints":[],"rm":false,"ro":false,"tran":"nvme","size":242872221696,"type":"part","model":null,"#,
        r#""fstype":"BitLocker","fsver":"2","parttypename":"Microsoft basic data","parttype":"ebd0a0a2-b9e5-4433-87c0-68b6b72699c7"}]}]}"#,
    ]
    .concat();
    let rows = parse_devices(&bitlocker, false);
    let row = rows.iter().find(|r| r.device == "/dev/nvme1n1p3").expect("the BitLocker partition is a rail row");
    assert!(row.locked, "a BitLocker partition reads as locked, so a click asks for its passphrase");
    assert_eq!(row.label, "Windows", "the locked BitLocker row carries its label");
    assert_eq!(crypto_name("BitLocker", "2"), "BitLocker", "the card names BitLocker as Windows does");
}
