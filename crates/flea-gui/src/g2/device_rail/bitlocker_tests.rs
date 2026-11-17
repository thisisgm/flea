use super::devices::{Devices, Effect};
use super::process::Outcome;
use super::unlock::{Card, Effect as CardEffect, Proc};

const LOCKED: &str = r#"{"blockdevices":[{"name":"nvme1n1p3","path":"/dev/nvme1n1p3","type":"part","fstype":"BitLocker","label":"Backup","size":500000000000,"mountpoints":[null]}]}"#;

#[test]
fn bitlocker_rail_activation_requests_unlock_instead_of_gio_mount() {
    for show_unmounted in [false, true] {
        let mut devices = Devices::new("fixture");
        devices.show_unmounted = show_unmounted;
        devices.poll(0);
        devices.list_exited(&Outcome {
            code: 0,
            stdout: LOCKED.into(),
            stderr: String::new(),
        });
        assert_eq!(devices.entries.len(), 1);
        assert!(matches!(
            &devices.activate(0)[..],
            [Effect::UnlockRequested { device, label, size: 500_000_000_000, fstype }]
                if device == "/dev/nvme1n1p3" && label == "Backup" && fstype == "BitLocker"
        ));
    }
}

#[test]
fn bitlocker_unlock_mounts_the_returned_cleartext_device() {
    let mut card = Card::new();
    card.open_for("/dev/nvme1n1p3", "Backup", 500_000_000_000, "BitLocker", "");
    card.type_text("fixture passphrase");
    card.submit(0);
    card.started(Proc::Unlock);
    let effects = card.exited(
        Proc::Unlock,
        &Outcome {
            code: 0,
            stdout: "Unlocked /dev/nvme1n1p3 as /dev/dm-7.\n".into(),
            stderr: String::new(),
        },
        1,
    );
    assert!(matches!(
        &effects[..],
        [CardEffect::Start(Proc::Mount, job)]
            if job.argv == ["udisksctl", "mount", "--block-device", "/dev/dm-7"]
    ));
}

/// udisks 2.11's answer when ntfs3 refuses a BitLocker volume's NTFS (issue #494).
const NTFS3_REFUSED: &str = "Error mounting /dev/dm-7: GDBus.Error:org.freedesktop.UDisks2.Error.Failed: Error mounting /dev/dm-7 at /run/media/u/Games: wrong fs type, bad option, bad superblock on /dev/mapper/Games, missing codepage or helper program, or other error\n";

fn bitlocker_mounting(fstype: &str) -> Card {
    let mut card = Card::new();
    card.open_for("/dev/nvme1n1p5", "Games", 3_756_913_590_272, fstype, "2");
    card.type_text("fixture passphrase");
    card.submit(0);
    card.started(Proc::Unlock);
    card.exited(
        Proc::Unlock,
        &Outcome {
            code: 0,
            stdout: "Unlocked /dev/nvme1n1p5 as /dev/dm-7.\n".into(),
            stderr: String::new(),
        },
        1,
    );
    card
}

fn refused() -> Outcome {
    Outcome {
        code: 1,
        stdout: String::new(),
        stderr: NTFS3_REFUSED.into(),
    }
}

#[test]
fn bitlocker_mount_ntfs3_refuses_is_retried_with_the_native_ntfs_driver() {
    // "LUKS": flea --unlock typed into before the listing named the drive.
    for fstype in ["BitLocker", "LUKS"] {
        let mut card = bitlocker_mounting(fstype);
        let effects = card.exited(Proc::Mount, &refused(), 2);
        assert!(
            matches!(
                &effects[..],
                [CardEffect::Start(Proc::Mount, job)]
                    if job.argv == ["udisksctl", "mount", "--block-device", "/dev/dm-7", "--filesystem-type", "ntfs"]
            ),
            "{fstype}"
        );
        let mounted = Outcome {
            code: 0,
            stdout: "Mounted /dev/dm-7 at /run/media/u/Games\n".into(),
            stderr: String::new(),
        };
        assert!(card.exited(Proc::Mount, &mounted, 3).iter().any(
            |e| matches!(e, CardEffect::Unlocked { path, .. } if path == "/run/media/u/Games")
        ));
    }
}

#[test]
fn bitlocker_mount_both_drivers_refuse_relocks_without_blaming_the_passphrase() {
    let mut card = bitlocker_mounting("BitLocker");
    card.exited(Proc::Mount, &refused(), 2);
    let effects = card.exited(Proc::Mount, &refused(), 3);
    assert!(
        matches!(&effects[..], [CardEffect::Start(Proc::Lock, job)] if job.argv.join(" ") == "udisksctl lock --block-device /dev/nvme1n1p5")
    );
    card.exited(
        Proc::Lock,
        &Outcome {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
        4,
    );
    assert!(
        !card.field.error,
        "the passphrase was right, so the field is not marked wrong"
    );
    assert_eq!(
        card.note.as_ref().map(|(n, _)| n.clone()),
        Some(flea::ui_logic_device_rail::unlock::mount_note("Games"))
    );
}
