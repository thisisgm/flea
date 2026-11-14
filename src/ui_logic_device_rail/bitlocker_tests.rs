use super::devices::{container_name, hidden, is_locked_node, parse_devices};
use super::lsblk::parse_nodes;
use super::menu::{rail_menu, MenuRow, RailAction};
use super::unlock::{backing_for, crypto_name, lock_argv};

fn body(fstype: &str, child: &str) -> String {
    format!(
        r#"{{"blockdevices":[{{"name":"nvme1n1p3","path":"/dev/nvme1n1p3","label":"Backup","mountpoints":[null],"rm":false,"size":500000000000,"type":"part","fstype":"{fstype}"{child}}}]}}"#
    )
}

const TYPES: &[&str] = &[
    "BitLocker",
    "bitlocker",
    "bItLoCkEr",
    "BITLOCKER",
    "crypto_LUKS",
    "CRYPTO_luks",
    "crypto_plain",
];

#[test]
fn encrypted_rail_rows_ignore_the_unmounted_setting() {
    for fstype in TYPES {
        for unmounted in [false, true] {
            let rows = parse_devices(&body(fstype, ""), unmounted);
            assert_eq!(rows.len(), 1, "{fstype}, unmounted={unmounted}");
            let row = &rows[0];
            assert!(row.locked, "{fstype} is a locked row");
            assert_eq!(row.glyph, "lock");
            assert_eq!(row.label, "Backup");
            assert_eq!(row.size, Some(500_000_000_000));
            assert_eq!(row.fstype, *fstype);
            let actions: Vec<_> = rail_menu(row).iter().filter_map(MenuRow::action).collect();
            assert_eq!(actions, vec![RailAction::UnlockVolume]);
        }
    }
}

#[test]
fn encrypted_classification_and_container_label_cover_bitlocker() {
    for fstype in TYPES {
        let nodes = parse_nodes(&body(fstype, "")).expect("fixture parses");
        assert!(is_locked_node(&nodes[0]), "{fstype} is locked");
        assert_eq!(
            container_name(&nodes[0]),
            "Backup",
            "{fstype} names its child"
        );
    }
}

#[test]
fn encrypted_hidden_exception_matches_luks() {
    for fstype in TYPES {
        let fixture = body(fstype, "").replace(
            r#""type":"part""#,
            r#""type":"part","parttype":"de94bba4-06d1-4d40-a16a-bfd50179d6ac""#,
        );
        let nodes = parse_nodes(&fixture).expect("fixture parses");
        assert!(
            !hidden(&nodes[0]),
            "{fstype} is an encrypted container even on a recovery partition"
        );
    }
}

#[test]
fn mounted_encrypted_containers_are_not_locked() {
    for fstype in TYPES {
        let fixture = body(fstype, "").replace("[null]", r#"["/media/direct"]"#);
        let nodes = parse_nodes(&fixture).expect("fixture parses");
        assert!(!is_locked_node(&nodes[0]));
    }
}

#[test]
fn unlocked_bitlocker_has_one_named_cleartext_child() {
    for fstype in TYPES {
        for mount in ["null", r#""/media/Backup""#] {
            let child = format!(
                r#", "children":[{{"name":"cleartext","path":"/dev/mapper/cleartext","type":"crypt","label":"Child label","fstype":"ntfs","size":499999000000,"mountpoints":[{mount}]}}]"#
            );
            let fixture = body(fstype, &child);
            for unmounted in [false, true] {
                let rows = parse_devices(&fixture, unmounted);
                assert_eq!(
                    rows.len(),
                    1,
                    "{fstype}, mount={mount}, unmounted={unmounted}"
                );
                assert_eq!(rows[0].device, "/dev/mapper/cleartext");
                assert_eq!(
                    rows[0].label, "Backup",
                    "container label wins over child label"
                );
                assert!(!rows[0].locked);
                assert!(!RailAction::UnlockVolume.available(&rows[0]));
            }
            assert_eq!(
                backing_for(&fixture, "/dev/mapper/cleartext"),
                "/dev/nvme1n1p3"
            );
            assert_eq!(
                lock_argv("/dev/nvme1n1p3"),
                ["udisksctl", "lock", "--block-device", "/dev/nvme1n1p3"]
            );
        }
    }
}

#[test]
fn bitlocker_card_uses_the_proper_name_in_any_case() {
    for fstype in &TYPES[..4] {
        assert_eq!(crypto_name(fstype, "2"), "BitLocker");
        assert_eq!(crypto_name(fstype, ""), "BitLocker");
    }
    assert_eq!(crypto_name("crypto_LUKS", "2"), "LUKS2");
    assert_eq!(crypto_name("crypto_LUKS", "1"), "LUKS1");
    assert_eq!(crypto_name("crypto_LUKS", ""), "LUKS");
    assert_eq!(crypto_name("crypto_plain", ""), "PLAIN");
}

#[test]
fn plain_and_unknown_volumes_keep_their_classification() {
    for fstype in [
        "ntfs",
        "ext4",
        "",
        "BitLocker2",
        "preBitLocker",
        "apfs",
        "cs_fvault2",
        "crypto",
    ] {
        let fixture = body(fstype, "");
        let nodes = parse_nodes(&fixture).expect("fixture parses");
        assert!(
            !is_locked_node(&nodes[0]),
            "{fstype} is not an unlockable signature"
        );
        assert_eq!(container_name(&nodes[0]), "");
    }
}
